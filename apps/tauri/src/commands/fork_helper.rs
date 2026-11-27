//! Fork-only: helper model (trackers, Notebook) commands.

use lettuce_app::api::{self, ApiContextSlot};
use lettuce_contracts::{self as dto, ApiError};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn helper_config_get(
    context: State<'_, ApiContextSlot>,
) -> Result<dto::HelperConfigView, ApiError> {
    let context = context.snapshot()?;
    api::helper_config_get(&context).await
}

#[tauri::command]
#[specta::specta]
pub async fn helper_config_set(
    context: State<'_, ApiContextSlot>,
    request: dto::HelperConfigUpdate,
) -> Result<dto::HelperConfigView, ApiError> {
    let context = context.snapshot()?;
    api::helper_config_set(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn helper_chat_get(
    context: State<'_, ApiContextSlot>,
    request: dto::HelperChatRequest,
) -> Result<dto::HelperChatView, ApiError> {
    let context = context.snapshot()?;
    api::helper_chat_get(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn helper_status(
    context: State<'_, ApiContextSlot>,
    request: dto::HelperChatRequest,
) -> Result<Option<dto::HelperStatusDto>, ApiError> {
    let context = context.snapshot()?;
    api::helper_status(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn helper_scope_set(
    context: State<'_, ApiContextSlot>,
    request: dto::HelperScopeUpdate,
) -> Result<(), ApiError> {
    let context = context.snapshot()?;
    api::helper_scope_set(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn helper_state_set(
    context: State<'_, ApiContextSlot>,
    request: dto::HelperStateUpdate,
) -> Result<dto::HelperChatView, ApiError> {
    let context = context.snapshot()?;
    api::helper_state_set(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn helper_message(
    context: State<'_, ApiContextSlot>,
    request: dto::HelperMessageRequest,
) -> Result<dto::HelperMessageView, ApiError> {
    let context = context.snapshot()?;
    api::helper_message(&context, request).await
}
