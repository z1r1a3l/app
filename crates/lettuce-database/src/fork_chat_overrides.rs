//! Fork-only per-chat overrides (migration 9007): whether the global or the
//! characters' regex scripts run in one chat, and the chat's own `{{key}}`
//! macros. The chat's own regex scripts live in `fork_regex_scripts` under
//! the `chat` scope kind.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Database, DatabaseError};

/// One chat macro: `{{key}}` becomes `value`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMacro {
    pub key: String,
    pub value: String,
}

/// A chat's overrides; the default changes nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatOverrides {
    pub skip_global: bool,
    pub skip_character: bool,
    pub macros: Vec<ChatMacro>,
}

/// The most macros one chat keeps, and the longest key and value.
pub const MAX_CHAT_MACROS: usize = 100;
const MAX_KEY_CHARS: usize = 64;
const MAX_VALUE_BYTES: usize = 64 * 1024;

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Keys are trimmed, without braces or `:`; empty and repeated keys drop.
fn clean(macros: &[ChatMacro]) -> Vec<ChatMacro> {
    let mut seen = std::collections::HashSet::new();
    macros
        .iter()
        .filter_map(|item| {
            let key: String = item.key.trim().trim_start_matches("{{").trim_end_matches("}}").trim().chars().take(MAX_KEY_CHARS).collect();
            if key.is_empty() || key.contains(['{', '}', ':']) || !seen.insert(key.to_lowercase()) {
                return None;
            }
            let mut value = item.value.clone();
            if value.len() > MAX_VALUE_BYTES {
                let mut end = MAX_VALUE_BYTES;
                while !value.is_char_boundary(end) {
                    end -= 1;
                }
                value.truncate(end);
            }
            Some(ChatMacro { key, value })
        })
        .take(MAX_CHAT_MACROS)
        .collect()
}

impl Database {
    pub fn fork_chat_overrides(&self, conversation_id: &str) -> Result<ChatOverrides, DatabaseError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT skip_global, skip_character, macros_json FROM fork_chat_overrides WHERE conversation_id=?1",
                [conversation_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()?;
        Ok(row
            .map(|(global, character, json)| ChatOverrides {
                skip_global: global != 0,
                skip_character: character != 0,
                macros: serde_json::from_str(&json).unwrap_or_default(),
            })
            .unwrap_or_default())
    }

    pub fn fork_chat_overrides_set(
        &self,
        conversation_id: &str,
        overrides: &ChatOverrides,
    ) -> Result<ChatOverrides, DatabaseError> {
        let stored = ChatOverrides {
            macros: clean(&overrides.macros),
            ..overrides.clone()
        };
        let json = serde_json::to_string(&stored.macros).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO fork_chat_overrides (conversation_id, skip_global, skip_character, macros_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (conversation_id) DO UPDATE SET skip_global = excluded.skip_global,
                 skip_character = excluded.skip_character, macros_json = excluded.macros_json,
                 updated_at = excluded.updated_at
             WHERE skip_global IS NOT excluded.skip_global OR skip_character IS NOT excluded.skip_character
                 OR macros_json IS NOT excluded.macros_json",
            params![
                conversation_id,
                i64::from(stored.skip_global),
                i64::from(stored.skip_character),
                json,
                now_millis()
            ],
        )?;
        Ok(stored)
    }
}

pub(crate) const OVERRIDE_COLUMNS: &[&str] = &[
    "conversation_id",
    "skip_global",
    "skip_character",
    "macros_json",
    "updated_at",
];

pub(crate) const SYNC_KIND: &str = "fork_chat_override";
pub(crate) const SYNC_SCHEMA: &str = "fork.chat_override";
pub(crate) const SYNC_VERSION: u32 = 1;

/// Overrides sync row by row, keyed by chat.
pub(crate) const SYNC_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_chat_overrides",
        key: &["conversation_id"],
        immutable: false,
        columns: &["skip_global", "skip_character", "macros_json", "updated_at"],
        revision: false,
        identity: &[],
        references: &[],
        scan_filter: None,
    };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_default_store_and_clean_macros() {
        let database = Database::open_in_memory().expect("db");
        assert_eq!(database.fork_chat_overrides("c1").expect("get"), ChatOverrides::default());
        let stored = database
            .fork_chat_overrides_set(
                "c1",
                &ChatOverrides {
                    skip_global: true,
                    skip_character: false,
                    macros: vec![
                        ChatMacro { key: " {{town}} ".into(), value: "Ривертон".into() },
                        ChatMacro { key: "TOWN".into(), value: "dup".into() },
                        ChatMacro { key: "".into(), value: "x".into() },
                        ChatMacro { key: "a::b".into(), value: "x".into() },
                    ],
                },
            )
            .expect("set");
        assert_eq!(stored.macros, vec![ChatMacro { key: "town".into(), value: "Ривертон".into() }]);
        let read = database.fork_chat_overrides("c1").expect("get");
        assert!(read.skip_global && !read.skip_character);
        assert_eq!(read.macros, stored.macros);
        assert_eq!(database.fork_chat_overrides("c2").expect("get"), ChatOverrides::default());
    }
}
