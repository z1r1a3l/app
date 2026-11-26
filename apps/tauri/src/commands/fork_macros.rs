//! Fork-only: macro variable, preview and dice commands.

use lettuce_app::api::{self, ApiContextSlot};
use lettuce_contracts::{self as dto, ApiError};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn macro_vars_get(
    context: State<'_, ApiContextSlot>,
    request: dto::MacroVarsRequest,
) -> Result<dto::MacroVarsView, ApiError> {
    let context = context.snapshot()?;
    api::macro_vars_get(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn macro_vars_replace(
    context: State<'_, ApiContextSlot>,
    request: dto::MacroVarsReplaceRequest,
) -> Result<dto::MacroVarsView, ApiError> {
    let context = context.snapshot()?;
    api::macro_vars_replace(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn macro_preview(
    context: State<'_, ApiContextSlot>,
    request: dto::MacroPreviewRequest,
) -> Result<dto::MacroPreviewResult, ApiError> {
    let context = context.snapshot()?;
    api::macro_preview(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn macro_message_rolls(
    context: State<'_, ApiContextSlot>,
    request: dto::MacroRollsRequest,
) -> Result<dto::MacroRollsView, ApiError> {
    let context = context.snapshot()?;
    api::macro_message_rolls(&context, request).await
}
