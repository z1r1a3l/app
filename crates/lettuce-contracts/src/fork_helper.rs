//! Fork-only: helper model (trackers + Notebook) DTOs.

use serde::{Deserialize, Serialize};

/// Which trackers run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperTrackersDto {
    pub scene: bool,
    pub characters: bool,
    pub bonds: bool,
    pub inventory: bool,
    pub npcs: bool,
    pub notebook: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperFieldDto {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperEntityDto {
    pub name: String,
    pub fields: Vec<HelperFieldDto>,
    /// Set on bonds the user froze; ignored on other entities.
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperHolderDto {
    pub owner: String,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperClueDto {
    pub text: String,
    pub message_id: String,
    /// `false`: the clue's words were not found in its reply.
    pub found: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperTruthDto {
    pub id: String,
    pub truth: String,
    pub hidden_from: String,
    pub revealed: bool,
    pub clues: Vec<HelperClueDto>,
}

/// The full tracked state of the shown path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperStateDto {
    pub scene: Vec<HelperFieldDto>,
    pub characters: Vec<HelperEntityDto>,
    pub bonds: Vec<HelperEntityDto>,
    pub inventory: Vec<HelperHolderDto>,
    pub npcs: Vec<HelperEntityDto>,
    pub notebook: Vec<HelperTruthDto>,
}

/// One change line as the app understood it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperChangeDto {
    /// `scene`, `characters`, `bonds`, `inventory`, `npcs` or `notebook`.
    pub tracker: String,
    /// `set`, `remove`, `item_add`, `item_remove`, `truth`, `clue`,
    /// `reveal` or `truth_remove`.
    pub kind: String,
    /// Character / NPC / owner name or truth id; empty for scene fields.
    pub subject: String,
    pub field: String,
    /// The new value, item, truth or clue text (empty: removed).
    pub value: String,
    pub found: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum HelperRunState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

/// The latest helper run of a chat (kept while the app runs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperStatusDto {
    pub job_id: String,
    pub message_id: String,
    pub state: HelperRunState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperConfigView {
    pub model_profile_id: Option<String>,
    /// `None`: the default prompt is used.
    pub prompt: Option<String>,
    pub default_prompt: String,
    /// Earlier messages the helper reads.
    pub history: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperConfigUpdate {
    pub model_profile_id: Option<String>,
    pub prompt: Option<String>,
    pub history: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperScopeDto {
    pub enabled: bool,
    pub trackers: HelperTrackersDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperChatRequest {
    pub conversation_id: String,
}

/// A chat's helper switches, state and latest run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperChatView {
    pub conversation_id: String,
    /// The direct chat's character (none for groups).
    pub character_id: Option<String>,
    pub character_name: Option<String>,
    /// The chat's own switch; `None` when it follows the character.
    pub chat: Option<HelperScopeDto>,
    pub character: Option<HelperScopeDto>,
    pub enabled: bool,
    /// The trackers that run (or would run once switched on).
    pub trackers: HelperTrackersDto,
    pub state: HelperStateDto,
    pub status: Option<HelperStatusDto>,
}

/// Sets a chat's or a character's switch; `scope` is `conversation` or
/// `character`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperScopeUpdate {
    pub scope: String,
    pub id: String,
    pub enabled: bool,
    pub trackers: HelperTrackersDto,
}

/// Saves the state editor: the difference becomes a manual edit anchored
/// to the latest reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperStateUpdate {
    pub conversation_id: String,
    pub state: HelperStateDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperMessageRequest {
    pub conversation_id: String,
    pub message_id: String,
}

/// What the helper read for the shown variant of one reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HelperMessageView {
    /// `false` when the helper has not read this variant.
    pub read: bool,
    pub changes: Vec<HelperChangeDto>,
    pub raw: String,
    /// Manual edits saved while this variant was the latest reply.
    pub edits: u32,
}
