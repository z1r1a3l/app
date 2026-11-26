//! Fork-only: regex script DTOs (SillyTavern-compatible scripts).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum RegexScopeKindDto {
    Global,
    Preset,
    Character,
}

/// A script list: the global one, a prompt preset's, or a character's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexScopeRef {
    pub kind: RegexScopeKindDto,
    /// The prompt or character id; ignored for the global scope.
    pub id: Option<String>,
}

/// One script. Placement numbers follow SillyTavern: 1 user message, 2 AI
/// reply, 3 slash command, 5 world info, 6 reasoning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexScriptDto {
    pub id: String,
    pub script_name: String,
    pub find_regex: String,
    pub replace_string: String,
    pub trim_strings: Vec<String>,
    pub placement: Vec<u8>,
    pub disabled: bool,
    /// Changes only what is shown in the chat.
    pub markdown_only: bool,
    /// Changes only what is sent to the model.
    pub prompt_only: bool,
    pub run_on_edit: bool,
    /// 0 none, 1 raw, 2 escaped macros in the find pattern.
    pub substitute_regex: u8,
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub min_depth: Option<i64>,
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub max_depth: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexScopeView {
    pub scope: RegexScopeRef,
    pub enabled: bool,
    /// A character card's scripts that wait for the user to allow them.
    pub pending: bool,
    pub scripts: Vec<RegexScriptDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexScopeRequest {
    pub scope: RegexScopeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexScriptsReplaceRequest {
    pub scope: RegexScopeRef,
    pub scripts: Vec<RegexScriptDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexScopeStateRequest {
    pub scope: RegexScopeRef,
    pub enabled: Option<bool>,
    pub pending: Option<bool>,
}

/// Parses a SillyTavern / Tavo / Lumiverse regex file, an ST preset or a
/// character card, without storing anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexImportParseRequest {
    pub json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexImportView {
    /// "scripts", "preset", "card" or "pack".
    pub kind: String,
    pub name: Option<String>,
    pub scripts: Vec<RegexScriptDto>,
}

/// Scripts the chat view runs on messages before it shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexConversationRequest {
    pub conversation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexConversationScripts {
    pub user_name: String,
    pub character_name: String,
    pub scripts: Vec<RegexScriptDto>,
    /// The chat's and the global macro variables, for `{{getvar}}` in
    /// display scripts.
    pub chat_vars: Vec<crate::MacroVarDto>,
    pub global_vars: Vec<crate::MacroVarDto>,
}

/// Runs scripts over a sample text (the editor's live tester).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexTestRequest {
    pub scripts: Vec<RegexScriptDto>,
    pub text: String,
    /// "display" or "prompt".
    pub target: String,
    pub placement: u8,
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub depth: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexTestResult {
    pub text: String,
    /// Scripts whose pattern did not compile, as `name: reason`.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct RegexScopeSummary {
    pub scope: RegexScopeRef,
    pub scripts: u32,
}
