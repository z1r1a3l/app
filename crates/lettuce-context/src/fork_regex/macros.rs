//! Macros inside regex replacements and trim strings, through the fork's
//! SillyTavern macro engine (`crate::fork_macros`). Variables are read-only
//! here: a `{{setvar}}` in a replacement changes nothing stored.

use std::collections::HashMap;

use crate::fork_macros::{MacroEnv, MacroRun};

/// Values the macros read.
#[derive(Debug, Clone, Default)]
pub struct BasicMacroValues {
    pub user: String,
    pub char: String,
    pub vars: HashMap<String, String>,
    pub global_vars: HashMap<String, String>,
}

/// Resolves the supported macros in `text`.
#[must_use]
pub fn substitute_basic_macros(text: &str, values: &BasicMacroValues) -> String {
    if !text.contains("{{") {
        return text.to_owned();
    }
    let env = MacroEnv {
        user: Some(values.user.clone()),
        char: Some(values.char.clone()),
        ..MacroEnv::default()
    };
    MacroRun::new(
        env,
        values.vars.clone().into_iter().collect(),
        values.global_vars.clone().into_iter().collect(),
    )
    .evaluate(text)
}
