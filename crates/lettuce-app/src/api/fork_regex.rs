//! Fork-only: regex script API (global, prompt preset and character lists).

use lettuce_context::fork_regex::{
    self as engine, RegexApplyContext, RegexScopeKey, RegexScript, RegexTarget,
};
use lettuce_contracts::{self as dto, ApiError, ApiErrorCode};
use lettuce_database::{RegexScopeKind, RegexScopeState};

use super::ApiContext;
use super::error::{api_error, invalid_field};

fn storage(error: impl std::fmt::Display) -> ApiError {
    api_error(ApiErrorCode::Internal, error.to_string())
}

fn scope(scope: &dto::RegexScopeRef) -> Result<(RegexScopeKind, String), ApiError> {
    let kind = match scope.kind {
        dto::RegexScopeKindDto::Global => return Ok((RegexScopeKind::Global, String::new())),
        dto::RegexScopeKindDto::Preset => RegexScopeKind::Preset,
        dto::RegexScopeKindDto::Character => RegexScopeKind::Character,
        dto::RegexScopeKindDto::Chat => RegexScopeKind::Chat,
    };
    let id = scope
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid_field("scope.id", "a preset or character scope needs an id"))?;
    uuid::Uuid::parse_str(id).map_err(|_| invalid_field("scope.id", "not an id"))?;
    Ok((kind, id.to_owned()))
}

pub(crate) fn script_dto(script: RegexScript) -> dto::RegexScriptDto {
    dto::RegexScriptDto {
        id: script.id,
        script_name: script.script_name,
        find_regex: script.find_regex,
        replace_string: script.replace_string,
        trim_strings: script.trim_strings,
        placement: script.placement,
        disabled: script.disabled,
        markdown_only: script.markdown_only,
        prompt_only: script.prompt_only,
        run_on_edit: script.run_on_edit,
        substitute_regex: script.substitute_regex,
        min_depth: script.min_depth,
        max_depth: script.max_depth,
    }
}

pub(crate) fn script_from_dto(script: dto::RegexScriptDto) -> RegexScript {
    RegexScript {
        id: script.id,
        script_name: script.script_name,
        find_regex: script.find_regex,
        replace_string: script.replace_string,
        trim_strings: script.trim_strings,
        placement: script.placement,
        disabled: script.disabled,
        markdown_only: script.markdown_only,
        prompt_only: script.prompt_only,
        run_on_edit: script.run_on_edit,
        substitute_regex: script.substitute_regex,
        min_depth: script.min_depth,
        max_depth: script.max_depth,
    }
}

fn view(
    database: &lettuce_database::Database,
    reference: dto::RegexScopeRef,
    kind: RegexScopeKind,
    id: &str,
) -> Result<dto::RegexScopeView, ApiError> {
    let state = database.fork_regex_scope_state(kind, id).map_err(storage)?;
    let scripts = database.fork_regex_list(kind, id).map_err(storage)?;
    Ok(dto::RegexScopeView {
        scope: reference,
        enabled: state.enabled,
        pending: state.pending,
        scripts: scripts.into_iter().map(script_dto).collect(),
    })
}

pub async fn regex_scope_get(
    context: &ApiContext,
    request: dto::RegexScopeRequest,
) -> Result<dto::RegexScopeView, ApiError> {
    let (kind, id) = scope(&request.scope)?;
    context
        .blocking(move |context| view(context.backend().database(), request.scope, kind, &id))
        .await
}

const MAX_SCRIPTS: usize = 2000;

pub async fn regex_scope_replace(
    context: &ApiContext,
    request: dto::RegexScriptsReplaceRequest,
) -> Result<dto::RegexScopeView, ApiError> {
    let (kind, id) = scope(&request.scope)?;
    if request.scripts.len() > MAX_SCRIPTS {
        return Err(invalid_field("scripts", "too many scripts"));
    }
    let scripts = request
        .scripts
        .into_iter()
        .map(script_from_dto)
        .collect::<Vec<_>>();
    context
        .blocking(move |context| {
            let database = context.backend().database();
            database
                .fork_regex_replace(kind, &id, &scripts)
                .map_err(storage)?;
            view(database, request.scope, kind, &id)
        })
        .await
}

pub async fn regex_scope_state_set(
    context: &ApiContext,
    request: dto::RegexScopeStateRequest,
) -> Result<dto::RegexScopeView, ApiError> {
    let (kind, id) = scope(&request.scope)?;
    context
        .blocking(move |context| {
            let database = context.backend().database();
            let current = database
                .fork_regex_scope_state(kind, &id)
                .map_err(storage)?;
            let next = RegexScopeState {
                enabled: request.enabled.unwrap_or(current.enabled),
                pending: request.pending.unwrap_or(current.pending),
            };
            database
                .fork_regex_set_scope_state(kind, &id, next)
                .map_err(storage)?;
            view(database, request.scope, kind, &id)
        })
        .await
}

/// Preset and character lists that hold scripts.
pub async fn regex_scopes_list(
    context: &ApiContext,
) -> Result<Vec<dto::RegexScopeSummary>, ApiError> {
    context
        .blocking(|context| {
            let scopes = context
                .backend()
                .database()
                .fork_regex_scopes()
                .map_err(storage)?;
            Ok(scopes
                .into_iter()
                .map(|(kind, id, count)| dto::RegexScopeSummary {
                    scope: scope_ref(kind, id),
                    scripts: u32::try_from(count).unwrap_or(u32::MAX),
                })
                .collect())
        })
        .await
}

fn scope_ref(kind: RegexScopeKind, id: String) -> dto::RegexScopeRef {
    dto::RegexScopeRef {
        kind: match kind {
            RegexScopeKind::Preset => dto::RegexScopeKindDto::Preset,
            RegexScopeKind::Character => dto::RegexScopeKindDto::Character,
            RegexScopeKind::Global => dto::RegexScopeKindDto::Global,
        },
        id: Some(id),
    }
}

/// Imported presets and character cards whose scripts wait for the user to
/// allow them.
pub async fn regex_pending_scopes(
    context: &ApiContext,
) -> Result<Vec<dto::RegexScopeRef>, ApiError> {
    context
        .blocking(|context| {
            Ok(context
                .backend()
                .database()
                .fork_regex_pending_scopes()
                .map_err(storage)?
                .into_iter()
                .map(|(kind, id)| scope_ref(kind, id))
                .collect())
        })
        .await
}

pub async fn regex_import_parse(
    _context: &ApiContext,
    request: dto::RegexImportParseRequest,
) -> Result<dto::RegexImportView, ApiError> {
    if request.json.len() > 64 * 1024 * 1024 {
        return Err(invalid_field("json", "file is too large"));
    }
    let parsed = engine::parse_regex_import(&request.json)
        .ok_or_else(|| invalid_field("json", "no regex scripts found in this file"))?;
    Ok(dto::RegexImportView {
        kind: parsed.kind.to_owned(),
        name: parsed.name,
        scripts: parsed.scripts.into_iter().map(script_dto).collect(),
    })
}

pub async fn regex_test(
    _context: &ApiContext,
    request: dto::RegexTestRequest,
) -> Result<dto::RegexTestResult, ApiError> {
    let target = match request.target.as_str() {
        "prompt" => RegexTarget::Prompt,
        _ => RegexTarget::Display,
    };
    let scripts = request
        .scripts
        .into_iter()
        .map(script_from_dto)
        .collect::<Vec<_>>();
    let errors = scripts
        .iter()
        .filter(|script| !script.disabled && !script.find_regex.is_empty())
        .filter(|script| engine::compile_find_regex(&script.find_regex).is_none())
        .map(|script| format!("{}: invalid pattern", script.script_name))
        .collect();
    let text = engine::apply_regex_scripts(
        &scripts,
        &request.text,
        RegexApplyContext {
            placement: request.placement,
            target,
            depth: request.depth,
        },
        &|value: &str| engine::substitute_basic_macros(value, &engine::BasicMacroValues::default()),
    );
    Ok(dto::RegexTestResult { text, errors })
}

/// The scripts a chat shows its messages through, with the names macros use.
pub async fn regex_conversation_scripts(
    context: &ApiContext,
    request: dto::RegexConversationRequest,
) -> Result<dto::RegexConversationScripts, ApiError> {
    let conversation_id: lettuce_types::ConversationId =
        super::error::parse_id(&request.conversation_id, "conversation_id")?;
    context
        .blocking(move |context| {
            let database = context.backend().database();
            let (mut key, macros) = match engine::remembered_scope(&conversation_id.to_string()) {
                Some(found) => found,
                None => live_scope(database, conversation_id),
            };
            key.conversation_id = Some(conversation_id.to_string());
            let scripts = database.fork_regex_effective(&key).map_err(storage)?;
            let (chat_vars, global_vars) =
                super::fork_macros::vars_dto(database, &conversation_id.to_string())?;
            Ok(dto::RegexConversationScripts {
                user_name: macros.user,
                character_name: macros.char,
                scripts: scripts.into_iter().map(script_dto).collect(),
                chat_vars,
                global_vars,
                chat_macros: super::fork_chat_regex::chat_macros(database, &conversation_id.to_string())
                    .into_iter()
                    .map(|(key, value)| dto::ChatMacroDto { key, value })
                    .collect(),
            })
        })
        .await
}

fn live_scope(
    database: &lettuce_database::Database,
    conversation_id: lettuce_types::ConversationId,
) -> (RegexScopeKey, engine::BasicMacroValues) {
    use lettuce_conversations::{ConversationKind, ConversationReader};
    let mut key = RegexScopeKey::default();
    let mut macros = engine::BasicMacroValues::default();
    let Ok(aggregate) = ConversationReader::get(database, conversation_id) else {
        return (key, macros);
    };
    match &aggregate.conversation.kind {
        ConversationKind::Direct(details) => {
            key.character_ids
                .push(details.character.source_id.to_string());
            if let Ok(Some(character)) =
                lettuce_characters::CharacterRepository::get(database, details.character.source_id)
            {
                macros.char = character.character.profile.name.clone();
            }
            let assembler = crate::ConversationContextAssembler::new(database);
            if let Ok(Some((prompt, _))) = assembler.live_direct_prompt(&aggregate.conversation) {
                key.prompt_id = Some(prompt.id.to_string());
            }
        }
        ConversationKind::Group(_) => {
            key.character_ids
                .extend(
                    aggregate
                        .conversation
                        .participants
                        .iter()
                        .filter_map(|participant| match participant.source {
                            lettuce_conversations::ParticipantSource::Character(id) => {
                                Some(id.to_string())
                            }
                            _ => None,
                        }),
                );
        }
    }
    (key, macros)
}

/// After an entity import: a SillyTavern preset's scripts join the new
/// prompt's list and a character card's scripts join the character's list;
/// both wait for the user to allow them (as SillyTavern asks for cards). Failures only log;
/// the import itself already succeeded.
pub(crate) fn after_entity_import(
    database: &lettuce_database::Database,
    kind: dto::FileKind,
    file_name: &str,
    bytes: &[u8],
    id: &str,
) {
    let found = match kind {
        dto::FileKind::PromptPreset => std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
            .map(|value| (RegexScopeKind::Preset, engine::scripts_from_preset(&value))),
        dto::FileKind::CharacterCard => lettuce_transfer::character_import_json(file_name, bytes)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .map(|value| (RegexScopeKind::Character, engine::scripts_from_card(&value))),
        _ => None,
    };
    let Some((scope_kind, scripts)) = found else {
        return;
    };
    if scripts.is_empty() {
        return;
    }
    let stored = database
        .fork_regex_replace(scope_kind, id, &scripts)
        .and_then(|()| {
            database.fork_regex_set_scope_state(
                scope_kind,
                id,
                RegexScopeState {
                    enabled: true,
                    pending: true,
                },
            )
        });
    match stored {
        Ok(()) => tracing::info!(count = scripts.len(), "fork: stored imported regex scripts"),
        Err(error) => tracing::warn!(%error, "fork: imported regex scripts could not be stored"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str =
        include_str!("../../../lettuce-context/tests/fixtures/vexa/vexa-v1.0-regex.json");

    #[test]
    fn imported_preset_and_card_scripts_wait_for_permission() {
        let backend = crate::AppBackend::open_in_memory(lettuce_types::TimestampMillis::new(1))
            .expect("backend");
        let database = backend.database();
        let scripts: serde_json::Value = serde_json::from_str(PACK).expect("pack");
        let preset = serde_json::json!({"prompts": [], "extensions": {"regex_scripts": scripts}});
        let prompt = uuid::Uuid::new_v4().to_string();
        after_entity_import(
            database,
            dto::FileKind::PromptPreset,
            "Vexa.json",
            preset.to_string().as_bytes(),
            &prompt,
        );
        assert_eq!(
            database
                .fork_regex_list(RegexScopeKind::Preset, &prompt)
                .expect("list")
                .len(),
            143
        );
        assert!(
            database
                .fork_regex_scope_state(RegexScopeKind::Preset, &prompt)
                .expect("state")
                .pending
        );

        let card = serde_json::json!({"spec": "chara_card_v2", "data": {"name": "A", "extensions": {"regex_scripts": [
            {"scriptName": "x", "findRegex": "/a/g", "replaceString": "b", "placement": [2], "markdownOnly": true}
        ]}}});
        let character = uuid::Uuid::new_v4().to_string();
        after_entity_import(
            database,
            dto::FileKind::CharacterCard,
            "a.json",
            card.to_string().as_bytes(),
            &character,
        );
        assert!(
            database
                .fork_regex_scope_state(RegexScopeKind::Character, &character)
                .expect("state")
                .pending
        );
        let mut pending = database.fork_regex_pending_scopes().expect("pending");
        pending.sort_by_key(|(kind, _)| *kind == RegexScopeKind::Character);
        assert_eq!(
            pending,
            vec![
                (RegexScopeKind::Preset, prompt.clone()),
                (RegexScopeKind::Character, character.clone())
            ]
        );
        let key = RegexScopeKey {
            prompt_id: Some(prompt.clone()),
            character_ids: vec![character],
            conversation_id: None,
        };
        let effective = database.fork_regex_effective(&key).expect("effective");
        assert!(effective.is_empty(), "pending scripts do not run");

        database
            .fork_regex_set_scope_state(
                RegexScopeKind::Preset,
                &prompt,
                RegexScopeState {
                    enabled: true,
                    pending: false,
                },
            )
            .expect("allow");
        let effective = database.fork_regex_effective(&key).expect("effective");
        assert_eq!(
            effective.len(),
            143,
            "an allowed preset runs, the card still waits"
        );
    }
}
