//! Fork-only storage for SillyTavern macro variables (migration 9002): chat
//! variables per conversation, global variables, and each generation's dice.
//! All three tables go into backups and sync row by row.

use std::sync::Arc;

use lettuce_context::fork_macros::{DiceRoll, MacroVarStore, VarChanges, Vars};
use rusqlite::{OptionalExtension, params};

use crate::{Database, DatabaseError};

/// Dice logs kept per chat; older ones are dropped.
const ROLL_LOGS_PER_CHAT: i64 = 500;

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn apply(
    transaction: &rusqlite::Transaction<'_>,
    conversation_id: Option<&str>,
    changes: &VarChanges,
    now: i64,
) -> Result<(), DatabaseError> {
    for (name, value) in changes {
        if name.is_empty() {
            continue;
        }
        match (conversation_id, value) {
            (Some(chat), Some(value)) => transaction.execute(
                "INSERT INTO fork_chat_vars (conversation_id, name, value, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (conversation_id, name) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
                 WHERE value IS NOT excluded.value",
                params![chat, name, value, now],
            )?,
            (Some(chat), None) => transaction.execute(
                "DELETE FROM fork_chat_vars WHERE conversation_id = ?1 AND name = ?2",
                params![chat, name],
            )?,
            (None, Some(value)) => transaction.execute(
                "INSERT INTO fork_global_vars (name, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (name) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
                 WHERE value IS NOT excluded.value",
                params![name, value, now],
            )?,
            (None, None) => {
                transaction.execute("DELETE FROM fork_global_vars WHERE name = ?1", params![name])?
            }
        };
    }
    Ok(())
}

impl Database {
    /// A chat's variables.
    pub fn fork_chat_vars(&self, conversation_id: &str) -> Result<Vars, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT name, value FROM fork_chat_vars WHERE conversation_id = ?1")?;
        let rows = statement
            .query_map(params![conversation_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<Vars>>()?;
        Ok(rows)
    }

    /// Global variables.
    pub fn fork_global_vars(&self) -> Result<Vars, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT name, value FROM fork_global_vars")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vars>>()?;
        Ok(rows)
    }

    /// Writes chat (`conversation_id`) and global changes in one transaction.
    pub fn fork_vars_apply(
        &self,
        conversation_id: &str,
        chat: &VarChanges,
        global: &VarChanges,
    ) -> Result<(), DatabaseError> {
        if chat.is_empty() && global.is_empty() {
            return Ok(());
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let now = now_millis();
        apply(&transaction, Some(conversation_id), chat, now)?;
        apply(&transaction, None, global, now)?;
        transaction.commit()?;
        Ok(())
    }

    /// Replaces a chat's variables (or the globals when `conversation_id`
    /// is `None`) with `vars`.
    pub fn fork_vars_replace(
        &self,
        conversation_id: Option<&str>,
        vars: &Vars,
    ) -> Result<(), DatabaseError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        // Only what differs is written, so unchanged rows keep their
        // `updated_at` and sync sees no change.
        let current = match conversation_id {
            Some(chat) => {
                let mut statement = transaction
                    .prepare("SELECT name, value FROM fork_chat_vars WHERE conversation_id = ?1")?;
                statement
                    .query_map(params![chat], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<rusqlite::Result<Vars>>()?
            }
            None => {
                let mut statement =
                    transaction.prepare("SELECT name, value FROM fork_global_vars")?;
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<rusqlite::Result<Vars>>()?
            }
        };
        let mut changes: VarChanges = current
            .keys()
            .filter(|name| !vars.contains_key(*name))
            .map(|name| (name.clone(), None))
            .collect();
        for (name, value) in vars {
            if current.get(name) != Some(value) {
                changes.insert(name.clone(), Some(value.clone()));
            }
        }
        apply(&transaction, conversation_id, &changes, now_millis())?;
        transaction.commit()?;
        Ok(())
    }

    /// Keeps one generation's dice; the oldest logs past the cap go.
    pub fn fork_rolls_record(
        &self,
        conversation_id: &str,
        fingerprint: &str,
        rolls: &[DiceRoll],
    ) -> Result<(), DatabaseError> {
        let json = serde_json::to_string(rolls).map_err(|error| {
            DatabaseError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO fork_macro_rolls (conversation_id, fingerprint, rolls_json, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (conversation_id, fingerprint) DO UPDATE SET rolls_json = excluded.rolls_json, created_at = excluded.created_at",
            params![conversation_id, fingerprint, json, now_millis()],
        )?;
        transaction.execute(
            "DELETE FROM fork_macro_rolls WHERE conversation_id = ?1 AND fingerprint NOT IN (
                 SELECT fingerprint FROM fork_macro_rolls WHERE conversation_id = ?1
                 ORDER BY created_at DESC LIMIT ?2)",
            params![conversation_id, ROLL_LOGS_PER_CHAT],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// The dice of the generation whose prompt had `fingerprint`.
    pub fn fork_rolls_for(
        &self,
        conversation_id: &str,
        fingerprint: &str,
    ) -> Result<Option<Vec<DiceRoll>>, DatabaseError> {
        let connection = self.connection()?;
        let json: Option<String> = connection
            .query_row(
                "SELECT rolls_json FROM fork_macro_rolls WHERE conversation_id = ?1 AND fingerprint = ?2",
                params![conversation_id, fingerprint],
                |row| row.get(0),
            )
            .optional()?;
        Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
    }

    /// Drops variables and dice of chats that no longer exist.
    pub fn fork_vars_prune(&self) -> Result<usize, DatabaseError> {
        let connection = self.connection()?;
        let vars = connection.execute(
            "DELETE FROM fork_chat_vars WHERE conversation_id NOT IN (SELECT id FROM conversations)",
            [],
        )?;
        let rolls = connection.execute(
            "DELETE FROM fork_macro_rolls WHERE conversation_id NOT IN (SELECT id FROM conversations)",
            [],
        )?;
        Ok(vars + rolls)
    }
}

const CHAT_VAR_COLUMNS: &[&str] = &["conversation_id", "name", "value", "updated_at"];
const GLOBAL_VAR_COLUMNS: &[&str] = &["name", "value", "updated_at"];
const ROLL_COLUMNS: &[&str] = &["conversation_id", "fingerprint", "rolls_json", "created_at"];

/// The macro tables as backup rows.
pub(crate) fn read_backup_in(
    transaction: &rusqlite::Transaction<'_>,
) -> rusqlite::Result<lettuce_transfer::ForkMacrosBackup> {
    use crate::legacy::legacy_import_backup_adapter::read_rows;
    Ok(lettuce_transfer::ForkMacrosBackup {
        chat_vars: read_rows(transaction, "fork_chat_vars", CHAT_VAR_COLUMNS, None)?,
        global_vars: read_rows(transaction, "fork_global_vars", GLOBAL_VAR_COLUMNS, None)?,
        rolls: read_rows(transaction, "fork_macro_rolls", ROLL_COLUMNS, None)?,
    })
}

/// Writes restored macro rows into the (empty) restore target.
pub(crate) fn insert_restored_in(
    transaction: &rusqlite::Transaction<'_>,
    backup: &lettuce_transfer::ForkMacrosBackup,
) -> rusqlite::Result<()> {
    use crate::legacy::legacy_import_backup_adapter::insert_rows;
    insert_rows(
        transaction,
        "fork_chat_vars",
        CHAT_VAR_COLUMNS,
        &backup.chat_vars,
    )?;
    insert_rows(
        transaction,
        "fork_global_vars",
        GLOBAL_VAR_COLUMNS,
        &backup.global_vars,
    )?;
    insert_rows(transaction, "fork_macro_rolls", ROLL_COLUMNS, &backup.rolls)
}

/// Sync kinds for the macro tables (fork-only; both devices run the fork).
pub(crate) const CHAT_VAR_SYNC_KIND: &str = "fork_chat_var";
pub(crate) const CHAT_VAR_SYNC_SCHEMA: &str = "fork.chat_var";
pub(crate) const GLOBAL_VAR_SYNC_KIND: &str = "fork_global_var";
pub(crate) const GLOBAL_VAR_SYNC_SCHEMA: &str = "fork.global_var";
pub(crate) const ROLL_SYNC_KIND: &str = "fork_macro_roll";
pub(crate) const ROLL_SYNC_SCHEMA: &str = "fork.macro_roll";
pub(crate) const SYNC_VERSION: u32 = 1;

const CONVERSATION: &[crate::sync::row_sync_adapter::RowReference] =
    &[crate::sync::row_sync_adapter::RowReference {
        column: "conversation_id",
        table: "conversations",
        required: true,
    }];

/// Chat variables sync row by row, keyed `conversation:name` (the name
/// comes last, so it may hold `:`), after their conversation.
pub(crate) const CHAT_VAR_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_chat_vars",
        key: &["conversation_id", "name"],
        immutable: false,
        columns: &["value", "updated_at"],
        revision: false,
        identity: &[],
        references: CONVERSATION,
        scan_filter: None,
    };

/// Global variables sync row by row, keyed by name.
pub(crate) const GLOBAL_VAR_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_global_vars",
        key: &["name"],
        immutable: false,
        columns: &["value", "updated_at"],
        revision: false,
        identity: &[],
        references: &[],
        scan_filter: None,
    };

/// Dice logs sync row by row, keyed `conversation:fingerprint`.
pub(crate) const ROLL_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_macro_rolls",
        key: &["conversation_id", "fingerprint"],
        immutable: false,
        columns: &["rolls_json", "created_at"],
        revision: false,
        identity: &[],
        references: CONVERSATION,
        scan_filter: None,
    };

/// Lets context assembly read and write variables through
/// `lettuce_context::fork_macros::set_var_store`.
#[derive(Debug, Clone)]
pub struct DatabaseMacroVarStore(pub Arc<Database>);

impl MacroVarStore for DatabaseMacroVarStore {
    fn chat_vars(&self, conversation_id: &str) -> Vars {
        self.0
            .fork_chat_vars(conversation_id)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "fork chat variables could not be read");
                Vars::new()
            })
    }

    fn global_vars(&self) -> Vars {
        self.0.fork_global_vars().unwrap_or_else(|error| {
            tracing::warn!(%error, "fork global variables could not be read");
            Vars::new()
        })
    }

    fn apply_changes(&self, conversation_id: &str, chat: &VarChanges, global: &VarChanges) {
        if let Err(error) = self.0.fork_vars_apply(conversation_id, chat, global) {
            tracing::warn!(%error, "fork variables could not be saved");
        }
    }

    fn record_rolls(&self, conversation_id: &str, fingerprint: &str, rolls: &[DiceRoll]) {
        if let Err(error) = self
            .0
            .fork_rolls_record(conversation_id, fingerprint, rolls)
        {
            tracing::warn!(%error, "fork dice log could not be saved");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_round_trip_per_chat_and_globally() {
        let database = Database::open_in_memory().expect("database");
        let mut chat = VarChanges::new();
        chat.insert("hp".into(), Some("10".into()));
        chat.insert("gone".into(), Some("x".into()));
        let mut global = VarChanges::new();
        global.insert("frankenSIM.mainTheme".into(), Some("Sola".into()));
        database
            .fork_vars_apply("c1", &chat, &global)
            .expect("apply");
        let mut later = VarChanges::new();
        later.insert("hp".into(), Some("9".into()));
        later.insert("gone".into(), None);
        database
            .fork_vars_apply("c1", &later, &VarChanges::new())
            .expect("apply");

        let vars = database.fork_chat_vars("c1").expect("vars");
        assert_eq!(vars.get("hp").map(String::as_str), Some("9"));
        assert!(!vars.contains_key("gone"));
        assert!(database.fork_chat_vars("c2").expect("other").is_empty());
        assert_eq!(
            database.fork_global_vars().expect("globals")["frankenSIM.mainTheme"],
            "Sola"
        );

        let mut replaced = Vars::new();
        replaced.insert("mood".into(), "calm".into());
        database
            .fork_vars_replace(Some("c1"), &replaced)
            .expect("replace");
        assert_eq!(database.fork_chat_vars("c1").expect("vars"), replaced);
        database
            .fork_vars_replace(None, &Vars::new())
            .expect("clear");
        assert!(database.fork_global_vars().expect("globals").is_empty());

        // c1 is no real conversation, so pruning drops it.
        assert_eq!(database.fork_vars_prune().expect("prune"), 1);
    }

    #[test]
    fn variables_and_dice_survive_a_backup_round_trip() {
        let database = Database::open_in_memory().expect("database");
        let mut chat = VarChanges::new();
        chat.insert("hp".into(), Some("10".into()));
        chat.insert("a:b".into(), Some("colon".into()));
        let mut global = VarChanges::new();
        global.insert("frankenSIM.mainTheme".into(), Some("Sola".into()));
        database
            .fork_vars_apply("c1", &chat, &global)
            .expect("apply");
        let rolls = vec![DiceRoll {
            formula: "2d6+1".into(),
            dice: vec![3, 4],
            modifier: 1,
            total: 8,
        }];
        database
            .fork_rolls_record("c1", "fp", &rolls)
            .expect("rolls");
        let (graph, restored) =
            crate::backup::restore_writer::tests::backup_round_trip(&database, |_| {});
        assert_eq!(graph.fork_macros.chat_vars.len(), 2);
        assert_eq!(graph.fork_macros.global_vars.len(), 1);
        assert_eq!(graph.fork_macros.rolls.len(), 1);
        let vars = restored.fork_chat_vars("c1").expect("vars");
        assert_eq!(vars.get("hp").map(String::as_str), Some("10"));
        assert_eq!(vars.get("a:b").map(String::as_str), Some("colon"));
        assert_eq!(
            restored.fork_global_vars().expect("globals")["frankenSIM.mainTheme"],
            "Sola"
        );
        assert_eq!(
            restored.fork_rolls_for("c1", "fp").expect("rolls"),
            Some(rolls)
        );
        let json = serde_json::to_value(&graph).expect("graph json");
        assert!(json.get("fork_macros").is_some());
    }

    #[test]
    fn a_backup_without_variables_leaves_the_field_out() {
        let database = Database::open_in_memory().expect("database");
        let graph = crate::backup::restore_writer::tests::assert_backup_round_trip(&database);
        let json = serde_json::to_value(&graph).expect("graph json");
        assert!(
            json.get("fork_macros").is_none(),
            "upstream builds can still read it"
        );
    }

    #[test]
    fn unchanged_values_keep_their_timestamps() {
        let database = Database::open_in_memory().expect("database");
        let stamps = || {
            database
                .connection()
                .expect("connection")
                .prepare("SELECT name, updated_at FROM fork_global_vars ORDER BY name")
                .expect("prepare")
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .expect("query")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("rows")
        };
        let mut global = VarChanges::new();
        global.insert("a".into(), Some("1".into()));
        global.insert("b".into(), Some("2".into()));
        database
            .fork_vars_apply("c1", &VarChanges::new(), &global)
            .expect("first");
        let before = stamps();
        std::thread::sleep(std::time::Duration::from_millis(5));
        database
            .fork_vars_apply("c1", &VarChanges::new(), &global)
            .expect("same again");
        let same: Vars = [
            ("a".to_owned(), "1".to_owned()),
            ("b".to_owned(), "2".to_owned()),
        ]
        .into();
        database
            .fork_vars_replace(None, &same)
            .expect("replace same");
        assert_eq!(stamps(), before);
    }

    #[test]
    fn dice_logs_are_found_by_fingerprint() {
        let database = Database::open_in_memory().expect("database");
        let rolls = vec![DiceRoll {
            formula: "1d20".into(),
            dice: vec![17],
            modifier: 0,
            total: 17,
        }];
        database
            .fork_rolls_record("c1", "abc", &rolls)
            .expect("record");
        assert_eq!(
            database.fork_rolls_for("c1", "abc").expect("read"),
            Some(rolls)
        );
        assert_eq!(database.fork_rolls_for("c1", "zzz").expect("read"), None);
        assert_eq!(database.fork_rolls_for("c2", "abc").expect("read"), None);
    }
}
