//! Fork-only: per-chat regex switches and `{{key}}` macros. The chat's own
//! scripts go through the regular regex scope API with the `chat` kind.

use lettuce_contracts::{self as dto, ApiError, ApiErrorCode};
use lettuce_database::{ForkChatMacro, ForkChatOverrides};

use super::ApiContext;
use super::error::api_error;

fn storage(error: impl std::fmt::Display) -> ApiError {
    api_error(ApiErrorCode::Internal, error.to_string())
}

fn view(conversation_id: String, overrides: ForkChatOverrides) -> dto::ChatRegexSettingsDto {
    dto::ChatRegexSettingsDto {
        conversation_id,
        skip_global: overrides.skip_global,
        skip_character: overrides.skip_character,
        macros: overrides
            .macros
            .into_iter()
            .map(|item| dto::ChatMacroDto { key: item.key, value: item.value })
            .collect(),
    }
}

pub async fn chat_regex_settings_get(
    context: &ApiContext,
    request: dto::RegexConversationRequest,
) -> Result<dto::ChatRegexSettingsDto, ApiError> {
    let conversation_id: lettuce_types::ConversationId =
        super::error::parse_id(&request.conversation_id, "conversation_id")?;
    context
        .blocking(move |context| {
            let id = conversation_id.to_string();
            let overrides = context.backend().database().fork_chat_overrides(&id).map_err(storage)?;
            Ok(view(id, overrides))
        })
        .await
}

pub async fn chat_regex_settings_set(
    context: &ApiContext,
    request: dto::ChatRegexSettingsDto,
) -> Result<dto::ChatRegexSettingsDto, ApiError> {
    let conversation_id: lettuce_types::ConversationId =
        super::error::parse_id(&request.conversation_id, "conversation_id")?;
    context
        .blocking(move |context| {
            let id = conversation_id.to_string();
            let overrides = ForkChatOverrides {
                skip_global: request.skip_global,
                skip_character: request.skip_character,
                macros: request
                    .macros
                    .into_iter()
                    .map(|item| ForkChatMacro { key: item.key, value: item.value })
                    .collect(),
            };
            let stored = context
                .backend()
                .database()
                .fork_chat_overrides_set(&id, &overrides)
                .map_err(storage)?;
            Ok(view(id, stored))
        })
        .await
}

/// The chat's macros as engine values (lower-case keys).
pub(crate) fn chat_macros(
    database: &lettuce_database::Database,
    conversation_id: &str,
) -> std::collections::BTreeMap<String, String> {
    database
        .fork_chat_overrides(conversation_id)
        .map(|overrides| {
            overrides
                .macros
                .into_iter()
                .map(|item| (item.key.to_lowercase(), item.value))
                .collect()
        })
        .unwrap_or_default()
}
