//! Fork-only: folded-chapter recall settings.

use lettuce_app::api::{self, ApiContextSlot};
use lettuce_contracts::{self as dto, ApiError};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn chapter_recall_config_get(
    context: State<'_, ApiContextSlot>,
) -> Result<dto::ChapterRecallConfigDto, ApiError> {
    let context = context.snapshot()?;
    api::chapter_recall_config_get(&context).await
}

#[tauri::command]
#[specta::specta]
pub async fn chapter_recall_config_set(
    context: State<'_, ApiContextSlot>,
    request: dto::ChapterRecallConfigDto,
) -> Result<dto::ChapterRecallConfigDto, ApiError> {
    let context = context.snapshot()?;
    api::chapter_recall_config_set(&context, request).await
}
