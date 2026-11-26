//! Process-wide access to the stored regex scripts.
//!
//! Context assembly is generic over many repository traits; threading one
//! more trait through every caller would touch a lot of upstream code. The
//! composition root registers the database here once instead, and assembly
//! asks it for the scripts of the chat it is building. Without a registered
//! source (tests, previews in isolation) no script runs.

use std::sync::{Arc, RwLock};

use super::{RegexApplyContext, RegexScript, RegexTarget, apply_regex_scripts};

/// The scopes a chat draws scripts from: global, its prompt preset and its
/// characters, in SillyTavern's order (global, preset, character).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegexScopeKey {
    pub prompt_id: Option<String>,
    pub character_ids: Vec<String>,
}

/// Resolves the enabled scripts for a chat.
pub trait RegexScriptSource: Send + Sync {
    fn scripts_for(&self, key: &RegexScopeKey) -> Vec<RegexScript>;
}

type Source = Arc<dyn RegexScriptSource>;

fn slot() -> &'static RwLock<Option<Source>> {
    static SLOT: RwLock<Option<Source>> = RwLock::new(None);
    &SLOT
}

pub fn set_regex_source(source: Source) {
    *slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(source);
}

pub fn clear_regex_source() {
    *slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

#[must_use]
pub fn regex_source() -> Option<Source> {
    slot()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Prompt-side regex for one chat: returns a function that rewrites a
/// history message's text given its placement and depth, or `None` when no
/// prompt-side script applies (so callers can skip the work entirely).
#[must_use]
pub fn prompt_regex_for(
    key: &RegexScopeKey,
    macros: super::BasicMacroValues,
) -> Option<impl Fn(&str, u8, i64) -> String + use<>> {
    let scripts = regex_source()?.scripts_for(key);
    let scripts: Vec<RegexScript> = scripts
        .into_iter()
        .filter(|script| !script.disabled && script.applies_to(RegexTarget::Prompt))
        .collect();
    if scripts.is_empty() {
        return None;
    }
    Some(move |text: &str, placement: u8, depth: i64| {
        apply_regex_scripts(
            &scripts,
            text,
            RegexApplyContext {
                placement,
                target: RegexTarget::Prompt,
                depth: Some(depth),
            },
            &|value: &str| super::substitute_basic_macros(value, &macros),
        )
    })
}

/// The last scope key and names prompt assembly used for each chat, so work
/// that reads history outside the prompt (memory, summaries) cleans it the
/// same way. Lost on restart; callers fall back to their own key.
fn remembered()
-> &'static RwLock<std::collections::HashMap<String, (RegexScopeKey, super::BasicMacroValues)>> {
    static SLOT: std::sync::OnceLock<
        RwLock<std::collections::HashMap<String, (RegexScopeKey, super::BasicMacroValues)>>,
    > = std::sync::OnceLock::new();
    SLOT.get_or_init(Default::default)
}

pub fn remember_scope(
    conversation_id: &str,
    key: &RegexScopeKey,
    macros: &super::BasicMacroValues,
) {
    let mut map = remembered()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if map.len() > 512 {
        map.clear();
    }
    map.insert(conversation_id.to_owned(), (key.clone(), macros.clone()));
}

#[must_use]
pub fn remembered_scope(conversation_id: &str) -> Option<(RegexScopeKey, super::BasicMacroValues)> {
    remembered()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(conversation_id)
        .cloned()
}
