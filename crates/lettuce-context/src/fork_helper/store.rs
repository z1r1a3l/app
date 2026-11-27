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

/// The database side, registered at start (prompt assembly has no
/// database). Every open database registers one; chat ids are unique, so a
/// lookup asks each live one in turn.
pub trait HelperStateSource: Send + Sync {
    /// The trackers that run in this chat; `None` when the helper is off.
    fn trackers(&self, conversation_id: &str, character_ids: &[String]) -> Option<Trackers>;
    /// Every stored change of the chat.
    fn deltas(&self, conversation_id: &str) -> Vec<StoredDelta>;
    /// `false` once its database is closed; it is then forgotten.
    fn alive(&self) -> bool {
        true
    }
}

type Source = Arc<dyn HelperStateSource>;

fn slot() -> &'static RwLock<Vec<Source>> {
    static SLOT: RwLock<Vec<Source>> = RwLock::new(Vec::new());
    &SLOT
}

/// Registers an open database's source.
pub fn set_state_source(source: Source) {
    let mut sources = slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    sources.retain(|known| known.alive());
    sources.push(source);
}

pub fn clear_state_source() {
    slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

/// The registered sources, asked in turn.
struct Sources(Vec<Source>);

impl HelperStateSource for Sources {
    fn trackers(&self, conversation_id: &str, character_ids: &[String]) -> Option<Trackers> {
        self.0
            .iter()
            .filter(|source| source.alive())
            .find_map(|source| source.trackers(conversation_id, character_ids))
    }

    fn deltas(&self, conversation_id: &str) -> Vec<StoredDelta> {
        self.0
            .iter()
            .filter(|source| source.alive())
            .map(|source| source.deltas(conversation_id))
            .find(|rows| !rows.is_empty())
            .unwrap_or_default()
    }
}

#[must_use]
pub fn state_source() -> Option<Source> {
    let sources = slot()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    (!sources.is_empty()).then(|| Arc::new(Sources(sources)) as Source)
}
