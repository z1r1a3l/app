//! Fork-only storage for the helper model (migration 9004): its settings,
//! the per-chat / per-character switches, and the tracker changes of each
//! reply variant. All three tables go into backups and sync row by row.


use lettuce_context::fork_helper::{DeltaSource, HelperStateSource, Op, StoredDelta, Trackers};
use rusqlite::{OptionalExtension, params};

use crate::{Database, DatabaseError};

/// Messages of story the helper reads before the latest exchange.
pub const DEFAULT_HELPER_HISTORY: u32 = 6;
pub const MAX_HELPER_HISTORY: u32 = 40;

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn json_error(error: serde_json::Error) -> DatabaseError {
    DatabaseError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

/// The helper's own settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperConfig {
    /// `None`: the app's default model.
    pub model_profile_id: Option<String>,
    /// `None`: the built-in default prompt.
    pub prompt: Option<String>,
    pub history: u32,
}

impl Default for HelperConfig {
    fn default() -> Self {
        Self {
            model_profile_id: None,
            prompt: None,
            history: DEFAULT_HELPER_HISTORY,
        }
    }
}

/// Where a switch lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperScopeKind {
    Character,
    Conversation,
}

impl HelperScopeKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Conversation => "conversation",
        }
    }
}

/// One stored switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelperScope {
    pub enabled: bool,
    pub trackers: Trackers,
}

/// A stored change set with the helper's raw answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperDeltaRow {
    pub delta: StoredDelta,
    pub raw: String,
}

fn source_str(source: DeltaSource) -> &'static str {
    match source {
        DeltaSource::Helper => "helper",
        DeltaSource::Manual => "manual",
    }
}

impl Database {
    pub fn fork_helper_config(&self) -> Result<HelperConfig, DatabaseError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT model_profile_id, prompt, history FROM fork_helper_config WHERE id = 'global'",
                [],
                |row| {
                    Ok(HelperConfig {
                        model_profile_id: row.get(0)?,
                        prompt: row.get(1)?,
                        history: u32::try_from(row.get::<_, i64>(2)?).unwrap_or(DEFAULT_HELPER_HISTORY),
                    })
                },
            )
            .optional()?;
        Ok(row.unwrap_or_default())
    }

    pub fn fork_helper_config_set(&self, config: &HelperConfig) -> Result<(), DatabaseError> {
        let connection = self.connection()?;
        let prompt = config.prompt.as_deref().filter(|p| !p.trim().is_empty());
        connection.execute(
            "INSERT INTO fork_helper_config (id, model_profile_id, prompt, history, updated_at)
             VALUES ('global', ?1, ?2, ?3, ?4)
             ON CONFLICT (id) DO UPDATE SET model_profile_id = excluded.model_profile_id,
                 prompt = excluded.prompt, history = excluded.history, updated_at = excluded.updated_at
             WHERE model_profile_id IS NOT excluded.model_profile_id OR prompt IS NOT excluded.prompt
                 OR history IS NOT excluded.history",
            params![
                config.model_profile_id,
                prompt,
                i64::from(config.history.min(MAX_HELPER_HISTORY)),
                now_millis()
            ],
        )?;
        Ok(())
    }

    pub fn fork_helper_scope(
        &self,
        kind: HelperScopeKind,
        id: &str,
    ) -> Result<Option<HelperScope>, DatabaseError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT enabled, trackers_json FROM fork_helper_scopes WHERE scope_kind = ?1 AND scope_id = ?2",
                params![kind.as_str(), id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(row.map(|(enabled, json)| HelperScope {
            enabled: enabled == 1,
            trackers: serde_json::from_str(&json).unwrap_or_default(),
        }))
    }

    pub fn fork_helper_scope_set(
        &self,
        kind: HelperScopeKind,
        id: &str,
        scope: HelperScope,
    ) -> Result<(), DatabaseError> {
        let json = serde_json::to_string(&scope.trackers).map_err(json_error)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO fork_helper_scopes (scope_kind, scope_id, enabled, trackers_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (scope_kind, scope_id) DO UPDATE SET enabled = excluded.enabled,
                 trackers_json = excluded.trackers_json, updated_at = excluded.updated_at
             WHERE enabled IS NOT excluded.enabled OR trackers_json IS NOT excluded.trackers_json",
            params![kind.as_str(), id, i64::from(scope.enabled), json, now_millis()],
        )?;
        Ok(())
    }

    /// The trackers that run in a chat: its own switch, else the first of
    /// its characters' switches that is on. `None` when the helper is off.
    pub fn fork_helper_effective(
        &self,
        conversation_id: &str,
        character_ids: &[String],
    ) -> Result<Option<Trackers>, DatabaseError> {
        if let Some(chat) =
            self.fork_helper_scope(HelperScopeKind::Conversation, conversation_id)?
        {
            return Ok(chat.enabled.then_some(chat.trackers));
        }
        for character in character_ids {
            if let Some(scope) = self.fork_helper_scope(HelperScopeKind::Character, character)?
                && scope.enabled
            {
                return Ok(Some(scope.trackers));
            }
        }
        Ok(None)
    }

    pub fn fork_helper_deltas(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<HelperDeltaRow>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, message_id, variant_key, source, ops_json, raw, created_at
             FROM fork_helper_deltas WHERE conversation_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = statement
            .query_map(params![conversation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .map(
                |(id, message_id, variant_key, source, ops, raw, created_at)| HelperDeltaRow {
                    delta: StoredDelta {
                        id,
                        message_id,
                        variant_key,
                        source: if source == "manual" {
                            DeltaSource::Manual
                        } else {
                            DeltaSource::Helper
                        },
                        // A row written by a newer fork with ops this build does
                        // not know reads as no changes rather than failing.
                        ops: serde_json::from_str::<Vec<Op>>(&ops).unwrap_or_default(),
                        created_at,
                    },
                    raw,
                },
            )
            .collect())
    }

    /// Stores the helper's read of one reply variant, replacing an earlier
    /// read of the same variant.
    pub fn fork_helper_put_read(
        &self,
        conversation_id: &str,
        message_id: &str,
        variant_key: &str,
        ops: &[Op],
        raw: &str,
    ) -> Result<String, DatabaseError> {
        self.put_delta(
            conversation_id,
            message_id,
            variant_key,
            DeltaSource::Helper,
            ops,
            raw,
        )
    }

    /// Stores a manual edit anchored to `message_id`/`variant_key` (empty:
    /// chat-wide).
    pub fn fork_helper_add_edit(
        &self,
        conversation_id: &str,
        message_id: &str,
        variant_key: &str,
        ops: &[Op],
    ) -> Result<String, DatabaseError> {
        self.put_delta(
            conversation_id,
            message_id,
            variant_key,
            DeltaSource::Manual,
            ops,
            "",
        )
    }

    fn put_delta(
        &self,
        conversation_id: &str,
        message_id: &str,
        variant_key: &str,
        source: DeltaSource,
        ops: &[Op],
        raw: &str,
    ) -> Result<String, DatabaseError> {
        let json = serde_json::to_string(ops).map_err(json_error)?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let now = now_millis();
        // Never older than what is already stored, so the fold keeps order
        // even when the clock steps back.
        let latest: Option<i64> = transaction.query_row(
            "SELECT MAX(created_at) FROM fork_helper_deltas WHERE conversation_id = ?1",
            params![conversation_id],
            |row| row.get(0),
        )?;
        let at = latest.map_or(now, |latest| now.max(latest + 1));
        if source == DeltaSource::Helper {
            transaction.execute(
                "DELETE FROM fork_helper_deltas WHERE conversation_id = ?1 AND message_id = ?2
                 AND variant_key = ?3 AND source = 'helper'",
                params![conversation_id, message_id, variant_key],
            )?;
        }
        transaction.execute(
            "INSERT INTO fork_helper_deltas
             (id, conversation_id, message_id, variant_key, source, ops_json, raw, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![id, conversation_id, message_id, variant_key, source_str(source), json, raw, at],
        )?;
        transaction.commit()?;
        Ok(id)
    }

    /// Drops switches and changes of chats that no longer exist.
    pub fn fork_helper_prune(&self) -> Result<usize, DatabaseError> {
        let connection = self.connection()?;
        let deltas = connection.execute(
            "DELETE FROM fork_helper_deltas WHERE conversation_id NOT IN (SELECT id FROM conversations)",
            [],
        )?;
        let scopes = connection.execute(
            "DELETE FROM fork_helper_scopes WHERE scope_kind = 'conversation'
             AND scope_id NOT IN (SELECT id FROM conversations)",
            [],
        )?;
        Ok(deltas + scopes)
    }
}

/// The database as the prompt assembler's state source.
#[derive(Debug, Clone)]
/// Holds its database weakly: a closed database drops out of the lookup.
pub struct DatabaseHelperStateSource(pub std::sync::Weak<Database>);

impl HelperStateSource for DatabaseHelperStateSource {
    fn trackers(&self, conversation_id: &str, character_ids: &[String]) -> Option<Trackers> {
        self.0
            .upgrade()?
            .fork_helper_effective(conversation_id, character_ids)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "fork helper switches could not be read");
                None
            })
    }

    fn deltas(&self, conversation_id: &str) -> Vec<StoredDelta> {
        let Some(database) = self.0.upgrade() else {
            return Vec::new();
        };
        database
            .fork_helper_deltas(conversation_id)
            .map(|rows| rows.into_iter().map(|row| row.delta).collect())
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "fork helper changes could not be read");
                Vec::new()
            })
    }

    fn alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

const CONFIG_COLUMNS: &[&str] = &["id", "model_profile_id", "prompt", "history", "updated_at"];
const SCOPE_COLUMNS: &[&str] = &[
    "scope_kind",
    "scope_id",
    "enabled",
    "trackers_json",
    "updated_at",
];
const DELTA_COLUMNS: &[&str] = &[
    "id",
    "conversation_id",
    "message_id",
    "variant_key",
    "source",
    "ops_json",
    "raw",
    "created_at",
    "updated_at",
];

/// The helper tables as backup rows.
pub(crate) fn read_backup_in(
    transaction: &rusqlite::Transaction<'_>,
) -> rusqlite::Result<lettuce_transfer::ForkHelperBackup> {
    use crate::legacy::legacy_import_backup_adapter::read_rows;
    Ok(lettuce_transfer::ForkHelperBackup {
        config: read_rows(transaction, "fork_helper_config", CONFIG_COLUMNS, None)?,
        scopes: read_rows(transaction, "fork_helper_scopes", SCOPE_COLUMNS, None)?,
        deltas: read_rows(transaction, "fork_helper_deltas", DELTA_COLUMNS, None)?,
    })
}

/// Writes restored helper rows into the (empty) restore target.
pub(crate) fn insert_restored_in(
    transaction: &rusqlite::Transaction<'_>,
    backup: &lettuce_transfer::ForkHelperBackup,
) -> rusqlite::Result<()> {
    use crate::legacy::legacy_import_backup_adapter::insert_rows;
    insert_rows(
        transaction,
        "fork_helper_config",
        CONFIG_COLUMNS,
        &backup.config,
    )?;
    insert_rows(
        transaction,
        "fork_helper_scopes",
        SCOPE_COLUMNS,
        &backup.scopes,
    )?;
    insert_rows(
        transaction,
        "fork_helper_deltas",
        DELTA_COLUMNS,
        &backup.deltas,
    )
}

/// Sync kinds for the helper tables (fork-only; both devices run the fork).
pub(crate) const CONFIG_SYNC_KIND: &str = "fork_helper_config";
pub(crate) const CONFIG_SYNC_SCHEMA: &str = "fork.helper_config";
pub(crate) const SCOPE_SYNC_KIND: &str = "fork_helper_scope";
pub(crate) const SCOPE_SYNC_SCHEMA: &str = "fork.helper_scope";
pub(crate) const DELTA_SYNC_KIND: &str = "fork_helper_delta";
pub(crate) const DELTA_SYNC_SCHEMA: &str = "fork.helper_delta";
pub(crate) const SYNC_VERSION: u32 = 1;

pub(crate) const CONFIG_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_helper_config",
        key: &["id"],
        immutable: false,
        columns: &["model_profile_id", "prompt", "history", "updated_at"],
        revision: false,
        identity: &[],
        references: &[],
        scan_filter: None,
    };

/// Switches sync keyed `kind:id`.
pub(crate) const SCOPE_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_helper_scopes",
        key: &["scope_kind", "scope_id"],
        immutable: false,
        columns: &["enabled", "trackers_json", "updated_at"],
        revision: false,
        identity: &[],
        references: &[],
        scan_filter: None,
    };

/// Changes sync row by row after their conversation.
pub(crate) const DELTA_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_helper_deltas",
        key: &["id"],
        immutable: false,
        columns: &[
            "conversation_id",
            "message_id",
            "variant_key",
            "source",
            "ops_json",
            "raw",
            "created_at",
            "updated_at",
        ],
        revision: false,
        identity: &[],
        references: &[crate::sync::row_sync_adapter::RowReference {
            column: "conversation_id",
            table: "conversations",
            required: true,
        }],
        scan_filter: None,
    };

#[cfg(test)]
mod tests {
    use lettuce_context::fork_helper::{Tracker, state_for_path};

    use super::*;

    fn open() -> Database {
        Database::open_in_memory().expect("database")
    }

    fn outfit(value: &str) -> Vec<Op> {
        vec![Op::Set {
            tracker: Tracker::Characters,
            entity: "Аква".into(),
            field: "Outfit".into(),
            value: value.into(),
        }]
    }

    #[test]
    fn config_defaults_and_round_trips() {
        let database = open();
        assert_eq!(
            database.fork_helper_config().expect("config"),
            HelperConfig::default()
        );
        let config = HelperConfig {
            model_profile_id: Some("model-1".into()),
            prompt: Some("my prompt".into()),
            history: 99,
        };
        database.fork_helper_config_set(&config).expect("set");
        let stored = database.fork_helper_config().expect("config");
        assert_eq!(stored.model_profile_id.as_deref(), Some("model-1"));
        assert_eq!(stored.prompt.as_deref(), Some("my prompt"));
        assert_eq!(stored.history, MAX_HELPER_HISTORY);
        database
            .fork_helper_config_set(&HelperConfig {
                prompt: Some("  ".into()),
                ..HelperConfig::default()
            })
            .expect("blank prompt");
        assert_eq!(database.fork_helper_config().expect("config").prompt, None);
    }

    #[test]
    fn off_by_default_and_chat_switch_wins_over_character() {
        let database = open();
        let characters = vec!["char-1".to_owned(), "char-2".to_owned()];
        assert_eq!(
            database
                .fork_helper_effective("chat-1", &characters)
                .expect("off"),
            None
        );
        let only_scene = Trackers {
            characters: false,
            bonds: false,
            inventory: false,
            npcs: false,
            notebook: false,
            ..Trackers::default()
        };
        database
            .fork_helper_scope_set(
                HelperScopeKind::Character,
                "char-2",
                HelperScope {
                    enabled: true,
                    trackers: only_scene,
                },
            )
            .expect("character on");
        assert_eq!(
            database
                .fork_helper_effective("chat-1", &characters)
                .expect("from character"),
            Some(only_scene)
        );
        database
            .fork_helper_scope_set(
                HelperScopeKind::Conversation,
                "chat-1",
                HelperScope {
                    enabled: false,
                    trackers: Trackers::default(),
                },
            )
            .expect("chat off");
        assert_eq!(
            database
                .fork_helper_effective("chat-1", &characters)
                .expect("chat off wins"),
            None
        );
        database
            .fork_helper_scope_set(
                HelperScopeKind::Conversation,
                "chat-1",
                HelperScope {
                    enabled: true,
                    trackers: Trackers::default(),
                },
            )
            .expect("chat on");
        assert_eq!(
            database
                .fork_helper_effective("chat-1", &characters)
                .expect("chat on"),
            Some(Trackers::default())
        );
    }

    #[test]
    fn a_new_read_replaces_the_variant_and_edits_stack() {
        let database = open();
        database
            .fork_helper_put_read("chat", "m1", "c:1", &outfit("платье"), "raw1")
            .expect("read");
        database
            .fork_helper_put_read("chat", "m1", "c:2", &outfit("доспех"), "raw2")
            .expect("other variant");
        database
            .fork_helper_put_read("chat", "m1", "c:1", &outfit("полотенце"), "raw3")
            .expect("again");
        database
            .fork_helper_add_edit("chat", "m1", "c:1", &outfit("халат"))
            .expect("edit");
        let rows = database.fork_helper_deltas("chat").expect("rows");
        assert_eq!(rows.len(), 3);
        assert!(
            rows.windows(2)
                .all(|pair| pair[0].delta.created_at < pair[1].delta.created_at)
        );
        let deltas = rows.iter().map(|row| row.delta.clone()).collect::<Vec<_>>();
        let state = state_for_path(&deltas, &[("m1".into(), "c:1".into())]);
        assert_eq!(state.characters[0].fields[0].value, "халат");
        let swiped = state_for_path(&deltas, &[("m1".into(), "c:2".into())]);
        assert_eq!(swiped.characters[0].fields[0].value, "доспех");
        // "chat" is no real conversation, so pruning drops its rows.
        database
            .fork_helper_scope_set(
                HelperScopeKind::Conversation,
                "chat",
                HelperScope {
                    enabled: true,
                    trackers: Trackers::default(),
                },
            )
            .expect("scope");
        assert_eq!(database.fork_helper_prune().expect("prune"), 4);
    }

    #[test]
    fn helper_rows_survive_a_backup_round_trip() {
        let database = open();
        database
            .fork_helper_config_set(&HelperConfig {
                model_profile_id: Some("m".into()),
                prompt: None,
                history: 4,
            })
            .expect("config");
        database
            .fork_helper_scope_set(
                HelperScopeKind::Character,
                "char-1",
                HelperScope {
                    enabled: true,
                    trackers: Trackers::default(),
                },
            )
            .expect("scope");
        database
            .fork_helper_put_read("chat", "m1", "c:1", &outfit("платье"), "raw")
            .expect("read");
        let (graph, restored) =
            crate::backup::restore_writer::tests::backup_round_trip(&database, |_| {});
        assert_eq!(graph.fork_helper.config.len(), 1);
        assert_eq!(graph.fork_helper.scopes.len(), 1);
        assert_eq!(graph.fork_helper.deltas.len(), 1);
        assert_eq!(restored.fork_helper_config().expect("config").history, 4);
        assert!(
            restored
                .fork_helper_scope(HelperScopeKind::Character, "char-1")
                .expect("scope")
                .is_some()
        );
        let rows = restored.fork_helper_deltas("chat").expect("rows");
        assert_eq!(rows[0].delta.ops, outfit("платье"));
        assert_eq!(rows[0].raw, "raw");
    }

    #[test]
    fn a_backup_without_helper_data_leaves_the_field_out() {
        let database = open();
        let graph = crate::backup::restore_writer::tests::assert_backup_round_trip(&database);
        let json = serde_json::to_value(&graph).expect("graph json");
        assert!(
            json.get("fork_helper").is_none(),
            "upstream builds can still read it"
        );
    }

    fn sync_all(from: &Database, to: &Database, at: i64) {
        use lettuce_sync::{IncomingBatchState, IncomingChangeRepository, LocalChangeJournal};
        use lettuce_types::TimestampMillis;
        from.journal_current_state(TimestampMillis::new(at))
            .expect("scan source");
        to.journal_current_state(TimestampMillis::new(at))
            .expect("scan target");
        loop {
            let batch = from
                .outbound_changes(
                    &to.local_frontier().expect("frontier"),
                    lettuce_sync::MAX_OUTBOUND_CHANGES,
                    lettuce_sync::MAX_OUTBOUND_PAYLOAD_BYTES,
                )
                .expect("outbound");
            if batch.changes.is_empty() {
                return;
            }
            let id = lettuce_types::OperationId::new();
            to.stage_incoming_batch(
                lettuce_sync::SyncDeviceId::new(),
                id,
                &lettuce_sync::canonical_batch_hash(&batch.changes),
                &batch.changes,
                TimestampMillis::new(at),
            )
            .expect("stage");
            assert_eq!(
                to.apply_incoming_batch(id, TimestampMillis::new(at))
                    .expect("apply")
                    .state,
                IncomingBatchState::Committed
            );
        }
    }

    #[test]
    fn settings_and_switches_sync_to_another_database() {
        let host = open();
        let client = open();
        host.fork_helper_config_set(&HelperConfig {
            model_profile_id: Some("m".into()),
            prompt: Some("p".into()),
            history: 3,
        })
        .expect("config");
        host.fork_helper_scope_set(
            HelperScopeKind::Character,
            "char-1",
            HelperScope {
                enabled: true,
                trackers: Trackers::default(),
            },
        )
        .expect("scope");
        sync_all(&host, &client, 2);
        assert_eq!(
            client
                .fork_helper_config()
                .expect("config")
                .prompt
                .as_deref(),
            Some("p")
        );
        assert_eq!(
            client
                .fork_helper_scope(HelperScopeKind::Character, "char-1")
                .expect("scope")
                .map(|s| s.enabled),
            Some(true)
        );
        client
            .fork_helper_scope_set(
                HelperScopeKind::Character,
                "char-1",
                HelperScope {
                    enabled: false,
                    trackers: Trackers::default(),
                },
            )
            .expect("off");
        sync_all(&client, &host, 3);
        assert_eq!(
            host.fork_helper_scope(HelperScopeKind::Character, "char-1")
                .expect("scope")
                .map(|s| s.enabled),
            Some(false)
        );
    }
}
