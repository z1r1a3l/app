//! Fork-only: regex script commands.

use lettuce_app::api::{self, ApiContextSlot};
use lettuce_contracts::{self as dto, ApiError};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn regex_scope_get(
    context: State<'_, ApiContextSlot>,
    request: dto::RegexScopeRequest,
) -> Result<dto::RegexScopeView, ApiError> {
    let context = context.snapshot()?;
    api::regex_scope_get(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn regex_scope_replace(
    context: State<'_, ApiContextSlot>,
    request: dto::RegexScriptsReplaceRequest,
) -> Result<dto::RegexScopeView, ApiError> {
    let context = context.snapshot()?;
    api::regex_scope_replace(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn regex_scope_state_set(
    context: State<'_, ApiContextSlot>,
    request: dto::RegexScopeStateRequest,
) -> Result<dto::RegexScopeView, ApiError> {
    let context = context.snapshot()?;
    api::regex_scope_state_set(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn regex_import_parse(
    context: State<'_, ApiContextSlot>,
    request: dto::RegexImportParseRequest,
) -> Result<dto::RegexImportView, ApiError> {
    let context = context.snapshot()?;
    api::regex_import_parse(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn regex_test(
    context: State<'_, ApiContextSlot>,
    request: dto::RegexTestRequest,
) -> Result<dto::RegexTestResult, ApiError> {
    let context = context.snapshot()?;
    api::regex_test(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn regex_conversation_scripts(
    context: State<'_, ApiContextSlot>,
    request: dto::RegexConversationRequest,
) -> Result<dto::RegexConversationScripts, ApiError> {
    let context = context.snapshot()?;
    api::regex_conversation_scripts(&context, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn regex_pending_characters(
    context: State<'_, ApiContextSlot>,
) -> Result<Vec<String>, ApiError> {
    let context = context.snapshot()?;
    api::regex_pending_characters(&context).await
}

#[tauri::command]
#[specta::specta]
pub async fn regex_scopes_list(
    context: State<'_, ApiContextSlot>,
) -> Result<Vec<dto::RegexScopeSummary>, ApiError> {
    let context = context.snapshot()?;
    api::regex_scopes_list(&context).await
}
