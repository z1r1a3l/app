//! Fork-only: SillyTavern macro variables, previews and dice DTOs.

use serde::{Deserialize, Serialize};

/// One variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MacroVarDto {
    pub name: String,
    pub value: String,
}

/// A chat's variables (empty without a chat) and the global ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MacroVarsView {
    pub conversation_id: Option<String>,
    pub chat: Vec<MacroVarDto>,
    pub global: Vec<MacroVarDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MacroVarsRequest {
    pub conversation_id: Option<String>,
}

/// Replaces one list: the chat's (`global` false) or the global one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MacroVarsReplaceRequest {
    pub conversation_id: Option<String>,
    pub global: bool,
    pub vars: Vec<MacroVarDto>,
}

/// One `{{roll}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct DiceRollDto {
    pub formula: String,
    pub dice: Vec<u32>,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub modifier: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub total: i64,
}

/// Evaluates prompt texts in order without saving anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MacroPreviewRequest {
    pub conversation_id: Option<String>,
    pub texts: Vec<String>,
    pub user_name: Option<String>,
    pub character_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MacroPreviewResult {
    pub texts: Vec<String>,
    pub rolls: Vec<DiceRollDto>,
    /// Variables after the texts ran.
    pub vars: MacroVarsView,
    /// Names the texts set or deleted.
    pub changed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MacroRollsRequest {
    pub message_id: String,
}

/// The dice a reply's prompt rolled (empty when it rolled none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MacroRollsView {
    pub rolls: Vec<DiceRollDto>,
}
