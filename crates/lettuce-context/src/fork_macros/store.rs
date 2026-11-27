//! Process-wide access to stored chat and global variables, registered by
//! the composition root (as the regex script source is), so context
//! assembly needs no new repository trait.

use std::sync::{Arc, RwLock};

use super::engine::{DiceRoll, VarChanges, Vars};

/// Where chat variables, global variables and dice logs live.
pub trait MacroVarStore: Send + Sync {
    fn chat_vars(&self, conversation_id: &str) -> Vars;
    fn global_vars(&self) -> Vars;
    /// The chat's own `{{key}}` macros, keys in lower case (fork: per-chat
    /// macros).
    fn chat_macros(&self, _conversation_id: &str) -> Vars {
        Vars::new()
    }
    /// Applies one run's writes (`None` deletes).
    fn apply_changes(&self, conversation_id: &str, chat: &VarChanges, global: &VarChanges);
    /// Keeps a generation's dice under its prompt fingerprint.
    fn record_rolls(&self, conversation_id: &str, fingerprint: &str, rolls: &[DiceRoll]);
}

type Store = Arc<dyn MacroVarStore>;

fn slot() -> &'static RwLock<Option<Store>> {
    static SLOT: RwLock<Option<Store>> = RwLock::new(None);
    &SLOT
}

pub fn set_var_store(store: Store) {
    *slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(store);
}

pub fn clear_var_store() {
    *slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

#[must_use]
pub fn var_store() -> Option<Store> {
    slot()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}
