//! Fork-only: SillyTavern-compatible regex scripts.
//!
//! Scripts keep SillyTavern's JSON shape (`scriptName`, `findRegex`,
//! `replaceString`, `markdownOnly`, `promptOnly`, ...) so ST, Tavo and preset
//! files round-trip without loss. The engine follows ST's
//! `public/scripts/extensions/regex/engine.js` and evaluates patterns with
//! ECMAScript semantics (`regress`), including variable-length lookbehind.
//!
//! Kept in its own module so upstream rebases touch as little as possible.

mod engine;
mod import;
mod macros;
mod provider;

pub use engine::{
    RegexApplyContext, RegexTarget, apply_regex_scripts, compile_find_regex, run_regex_script,
};
pub use import::{RegexImport, parse_regex_import, scripts_from_card, scripts_from_preset};
pub use macros::{BasicMacroValues, substitute_basic_macros};
pub use provider::{
    RegexScriptSource, RegexScopeKey, clear_regex_source, prompt_regex_for, regex_source,
    set_regex_source,
};

use serde::{Deserialize, Serialize};

/// Where a script applies, using SillyTavern's numbers.
pub mod placement {
    pub const USER_INPUT: u8 = 1;
    pub const AI_OUTPUT: u8 = 2;
    pub const SLASH_COMMAND: u8 = 3;
    pub const WORLD_INFO: u8 = 5;
    pub const REASONING: u8 = 6;
}

/// One regex script in SillyTavern's JSON shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegexScript {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub script_name: String,
    #[serde(default)]
    pub find_regex: String,
    #[serde(default)]
    pub replace_string: String,
    #[serde(default)]
    pub trim_strings: Vec<String>,
    #[serde(default, deserialize_with = "lenient_placement")]
    pub placement: Vec<u8>,
    #[serde(default, deserialize_with = "lenient_bool")]
    pub disabled: bool,
    #[serde(default, deserialize_with = "lenient_bool")]
    pub markdown_only: bool,
    #[serde(default, deserialize_with = "lenient_bool")]
    pub prompt_only: bool,
    #[serde(default, deserialize_with = "lenient_bool")]
    pub run_on_edit: bool,
    #[serde(default, deserialize_with = "lenient_u8")]
    pub substitute_regex: u8,
    #[serde(default, deserialize_with = "lenient_depth")]
    pub min_depth: Option<i64>,
    #[serde(default, deserialize_with = "lenient_depth")]
    pub max_depth: Option<i64>,
}

impl Default for RegexScript {
    fn default() -> Self {
        Self {
            id: String::new(),
            script_name: String::new(),
            find_regex: String::new(),
            replace_string: String::new(),
            trim_strings: Vec::new(),
            placement: vec![placement::AI_OUTPUT],
            disabled: false,
            markdown_only: true,
            prompt_only: false,
            run_on_edit: true,
            substitute_regex: 0,
            min_depth: None,
            max_depth: None,
        }
    }
}

fn lenient_bool<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(match value {
        serde_json::Value::Bool(b) => b,
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        serde_json::Value::String(s) => matches!(s.trim(), "true" | "1"),
        _ => false,
    })
}

fn lenient_u8<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(match value {
        serde_json::Value::Number(n) => n.as_u64().map_or(0, |n| n.min(255) as u8),
        serde_json::Value::String(s) => s.trim().parse().unwrap_or(0),
        serde_json::Value::Bool(true) => 1,
        _ => 0,
    })
}

fn lenient_depth<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(match value {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

fn lenient_placement<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let one = |v: &serde_json::Value| -> Option<u8> {
        match v {
            serde_json::Value::Number(n) => n.as_u64().map(|n| n.min(255) as u8),
            serde_json::Value::String(s) => match s.trim() {
                "user_input" | "userInput" => Some(placement::USER_INPUT),
                "ai_output" | "aiOutput" => Some(placement::AI_OUTPUT),
                "slash_command" | "slashCommand" => Some(placement::SLASH_COMMAND),
                "world_info" | "worldInfo" => Some(placement::WORLD_INFO),
                "reasoning" => Some(placement::REASONING),
                other => other.parse().ok(),
            },
            _ => None,
        }
    };
    Ok(match &value {
        serde_json::Value::Array(items) => items.iter().filter_map(one).collect(),
        serde_json::Value::Null => Vec::new(),
        other => one(other).into_iter().collect(),
    })
}

impl RegexScript {
    /// Without a stored-text mode in this fork, a script that is neither
    /// display-only nor prompt-only (ST: "alter the chat file") applies to
    /// both the display and the outgoing prompt, which has the same effect.
    #[must_use]
    pub fn applies_to(&self, target: RegexTarget) -> bool {
        match target {
            RegexTarget::Display => self.markdown_only || !self.prompt_only,
            RegexTarget::Prompt => self.prompt_only || !self.markdown_only,
        }
    }
}

#[cfg(test)]
mod tests;
