//! Fork-only storage for SillyTavern-style regex scripts (migration 9001).
//!
//! Scripts are kept per scope (global, a prompt preset, a character) as
//! SillyTavern JSON, in order. A scope can be switched off as a whole; a
//! character card's embedded scripts start `pending` until the user allows
//! them, as SillyTavern asks before running a card's scripts.

use std::sync::Arc;

use lettuce_context::fork_regex::{RegexScopeKey, RegexScript, RegexScriptSource};
use rusqlite::{OptionalExtension, params};

use crate::{Database, DatabaseError};

/// Which list a script belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegexScopeKind {
    Global,
    Preset,
    Character,
}

impl RegexScopeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Preset => "preset",
            Self::Character => "character",
        }
    }
}

/// A scope's switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegexScopeState {
    pub enabled: bool,
    pub pending: bool,
}

impl Default for RegexScopeState {
    fn default() -> Self {
        Self {
            enabled: true,
            pending: false,
        }
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn scope_id(kind: RegexScopeKind, id: &str) -> &str {
    if kind == RegexScopeKind::Global {
        ""
    } else {
        id
    }
}

impl Database {
    /// The scope's scripts in order.
    pub fn fork_regex_list(
        &self,
        kind: RegexScopeKind,
        id: &str,
    ) -> Result<Vec<RegexScript>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT script_json FROM fork_regex_scripts WHERE scope_kind=?1 AND scope_id=?2 ORDER BY position, id",
        )?;
        let rows = statement
            .query_map(params![kind.as_str(), scope_id(kind, id)], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .iter()
            .filter_map(|json| serde_json::from_str::<RegexScript>(json).ok())
            .collect())
    }

    /// Replaces the scope's scripts with `scripts`, in that order.
    pub fn fork_regex_replace(
        &self,
        kind: RegexScopeKind,
        id: &str,
        scripts: &[RegexScript],
    ) -> Result<(), DatabaseError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let scope = scope_id(kind, id);
        transaction.execute(
            "DELETE FROM fork_regex_scripts WHERE scope_kind=?1 AND scope_id=?2",
            params![kind.as_str(), scope],
        )?;
        let now = now_millis();
        let mut seen = std::collections::HashSet::new();
        for (position, script) in scripts.iter().enumerate() {
            let mut script = script.clone();
            if script.id.trim().is_empty() || !seen.insert(script.id.clone()) {
                script.id = format!("{}-{position}", uuid::Uuid::new_v4());
                seen.insert(script.id.clone());
            }
            let json = serde_json::to_string(&script).map_err(|_| rusqlite::Error::InvalidQuery)?;
            transaction.execute(
                "INSERT INTO fork_regex_scripts (scope_kind,scope_id,id,position,script_json,updated_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![kind.as_str(), scope, script.id, i64::try_from(position).unwrap_or(i64::MAX), json, now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// The scope's switches; a scope never set is enabled and not pending.
    pub fn fork_regex_scope_state(
        &self,
        kind: RegexScopeKind,
        id: &str,
    ) -> Result<RegexScopeState, DatabaseError> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT enabled,pending FROM fork_regex_scopes WHERE scope_kind=?1 AND scope_id=?2",
                params![kind.as_str(), scope_id(kind, id)],
                |row| {
                    Ok(RegexScopeState {
                        enabled: row.get::<_, i64>(0)? != 0,
                        pending: row.get::<_, i64>(1)? != 0,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn fork_regex_set_scope_state(
        &self,
        kind: RegexScopeKind,
        id: &str,
        state: RegexScopeState,
    ) -> Result<(), DatabaseError> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO fork_regex_scopes (scope_kind,scope_id,enabled,pending,updated_at) VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(scope_kind,scope_id) DO UPDATE SET enabled=excluded.enabled, pending=excluded.pending, updated_at=excluded.updated_at",
            params![kind.as_str(), scope_id(kind, id), i64::from(state.enabled), i64::from(state.pending), now_millis()],
        )?;
        Ok(())
    }

    /// Character scopes whose embedded scripts still wait for a decision.
    pub fn fork_regex_pending_characters(&self) -> Result<Vec<String>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT s.scope_id FROM fork_regex_scopes s WHERE s.scope_kind='character' AND s.pending=1 \
             AND EXISTS (SELECT 1 FROM fork_regex_scripts r WHERE r.scope_kind='character' AND r.scope_id=s.scope_id) ORDER BY s.updated_at DESC",
        )?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Every non-global scope that holds scripts, with its script count.
    pub fn fork_regex_scopes(&self) -> Result<Vec<(RegexScopeKind, String, usize)>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT scope_kind, scope_id, count(*) FROM fork_regex_scripts WHERE scope_kind<>'global' GROUP BY scope_kind, scope_id ORDER BY scope_kind, scope_id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(kind, id, count)| {
                let kind = match kind.as_str() {
                    "preset" => RegexScopeKind::Preset,
                    "character" => RegexScopeKind::Character,
                    _ => return None,
                };
                Some((kind, id, usize::try_from(count).unwrap_or(0)))
            })
            .collect())
    }

    /// Every script that runs for a chat: global, then its preset, then its
    /// characters (SillyTavern's order), skipping switched-off or pending
    /// scopes and disabled scripts.
    pub fn fork_regex_effective(
        &self,
        key: &RegexScopeKey,
    ) -> Result<Vec<RegexScript>, DatabaseError> {
        let mut scopes = vec![(RegexScopeKind::Global, String::new())];
        if let Some(prompt) = &key.prompt_id {
            scopes.push((RegexScopeKind::Preset, prompt.clone()));
        }
        for character in &key.character_ids {
            scopes.push((RegexScopeKind::Character, character.clone()));
        }
        let mut out = Vec::new();
        for (kind, id) in scopes {
            let state = self.fork_regex_scope_state(kind, &id)?;
            if !state.enabled || state.pending {
                continue;
            }
            out.extend(
                self.fork_regex_list(kind, &id)?
                    .into_iter()
                    .filter(|script| !script.disabled),
            );
        }
        Ok(out)
    }
}

/// Lets context assembly read the scripts through
/// `lettuce_context::fork_regex::set_regex_source`.
#[derive(Debug, Clone)]
pub struct DatabaseRegexSource(pub Arc<Database>);

impl RegexScriptSource for DatabaseRegexSource {
    fn scripts_for(&self, key: &RegexScopeKey) -> Vec<RegexScript> {
        self.0.fork_regex_effective(key).unwrap_or_else(|error| {
            tracing::warn!(%error, "fork regex scripts could not be read");
            Vec::new()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(name: &str) -> RegexScript {
        RegexScript {
            script_name: name.to_owned(),
            find_regex: "/a/g".to_owned(),
            ..RegexScript::default()
        }
    }

    #[test]
    fn scopes_store_order_and_resolve_in_sillytavern_order() {
        let database = Database::open_in_memory().expect("database");
        database
            .fork_regex_replace(RegexScopeKind::Global, "", &[script("g1"), script("g2")])
            .expect("global");
        database
            .fork_regex_replace(RegexScopeKind::Preset, "p1", &[script("p")])
            .expect("preset");
        database
            .fork_regex_replace(RegexScopeKind::Character, "c1", &[script("c")])
            .expect("character");
        let names =
            |list: Vec<RegexScript>| list.into_iter().map(|s| s.script_name).collect::<Vec<_>>();
        assert_eq!(
            names(
                database
                    .fork_regex_list(RegexScopeKind::Global, "x")
                    .expect("list")
            ),
            ["g1", "g2"]
        );
        let key = RegexScopeKey {
            prompt_id: Some("p1".into()),
            character_ids: vec!["c1".into()],
        };
        assert_eq!(
            names(database.fork_regex_effective(&key).expect("effective")),
            ["g1", "g2", "p", "c"]
        );

        database
            .fork_regex_set_scope_state(
                RegexScopeKind::Character,
                "c1",
                RegexScopeState {
                    enabled: true,
                    pending: true,
                },
            )
            .expect("pending");
        assert_eq!(
            names(database.fork_regex_effective(&key).expect("effective")),
            ["g1", "g2", "p"]
        );
        assert_eq!(
            database.fork_regex_pending_characters().expect("pending"),
            ["c1"]
        );
        database
            .fork_regex_set_scope_state(
                RegexScopeKind::Preset,
                "p1",
                RegexScopeState {
                    enabled: false,
                    pending: false,
                },
            )
            .expect("off");
        assert_eq!(
            names(database.fork_regex_effective(&key).expect("effective")),
            ["g1", "g2"]
        );

        database
            .fork_regex_replace(RegexScopeKind::Global, "", &[script("g2"), script("g1")])
            .expect("reorder");
        assert_eq!(
            names(
                database
                    .fork_regex_list(RegexScopeKind::Global, "")
                    .expect("list")
            ),
            ["g2", "g1"]
        );
    }
}
