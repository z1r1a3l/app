//! Fork-only: per-chat regex switches and macros.

use lettuce_app::api::{self, ApiContextSlot};
use lettuce_contracts::{self as dto, ApiError};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn chat_regex_settings_get(
    context: State<'_, ApiContextSlot>,
    request: dto::RegexConversationRequest,
) -> Result<dto::ChatRegexSettingsDto, ApiError> {
    let context = context.snapshot()?;
    api::chat_regex_settings_get(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn chat_regex_settings_set(
    context: State<'_, ApiContextSlot>,
    request: dto::ChatRegexSettingsDto,
) -> Result<dto::ChatRegexSettingsDto, ApiError> {
    let context = context.snapshot()?;
    api::chat_regex_settings_set(&context, request).await
}
