//! Where stored changes live, and how a chat path's state is folded.

use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use super::model::{HelperState, Op, Trackers, apply};

/// Who wrote a stored change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaSource {
    Helper,
    Manual,
}

/// One stored change set: a reply variant's helper read, or a manual edit
/// anchored to the reply shown when it was made (empty ids: chat-wide).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDelta {
    pub id: String,
    pub message_id: String,
    /// `c:<candidate>` or `r:<revision>`: the variant the change belongs to.
    pub variant_key: String,
    pub source: DeltaSource,
    pub ops: Vec<Op>,
    pub created_at: i64,
}

/// The state of a path: chat-wide edits first, then each shown message's
/// changes (only those of its shown variant) in timeline order.
#[must_use]
pub fn state_for_path(rows: &[StoredDelta], path: &[(String, String)]) -> HelperState {
    let mut state = HelperState::default();
    let mut ordered = rows
        .iter()
        .filter(|row| row.message_id.is_empty())
        .collect::<Vec<_>>();
    ordered.sort_by_key(|row| row.created_at);
    for row in ordered {
        apply(&mut state, &row.ops, "");
    }
    for (message_id, variant_key) in path {
        let mut rows = rows
            .iter()
            .filter(|row| &row.message_id == message_id && &row.variant_key == variant_key)
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| row.created_at);
        for row in rows {
            apply(&mut state, &row.ops, message_id);
        }
    }
    state
}

/// The database side, set once at start (prompt assembly has no database).
pub trait HelperStateSource: Send + Sync {
    /// The trackers that run in this chat; `None` when the helper is off.
    fn trackers(&self, conversation_id: &str, character_ids: &[String]) -> Option<Trackers>;
    /// Every stored change of the chat.
    fn deltas(&self, conversation_id: &str) -> Vec<StoredDelta>;
}

type Source = Arc<dyn HelperStateSource>;

fn slot() -> &'static RwLock<Option<Source>> {
    static SLOT: RwLock<Option<Source>> = RwLock::new(None);
    &SLOT
}

pub fn set_state_source(source: Source) {
    *slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(source);
}

pub fn clear_state_source() {
    *slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

#[must_use]
pub fn state_source() -> Option<Source> {
    slot()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}
