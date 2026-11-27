//! Fork-only: helper model API (switches, settings, state editor, per-reply
//! reads and the latest run's status) and the post-reply start.

use std::collections::HashMap;
use std::sync::Mutex;

use lettuce_context::fork_helper::{
    self as helper, Clue, DeltaSource, Entity, Field, HelperState, Holder, Op, Trackers, Truth,
};
use lettuce_contracts::{self as dto, ApiError, ApiErrorCode};
use lettuce_conversations::{ConversationKind, ConversationReader, MessageRole};
use lettuce_database::{
    ForkHelperConfig, ForkHelperScope, ForkHelperScopeKind, MAX_HELPER_HISTORY,
};
use lettuce_types::{ConversationId, MessageId};

use super::ApiContext;
use super::error::{IntoApiError, api_error, invalid_field, parse_id};
use crate::generation::fork_prompt_helper::{character_ids, path_of, variant_key};

const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_STATE_BYTES: usize = 512 * 1024;

fn storage(error: impl std::fmt::Display) -> ApiError {
    api_error(ApiErrorCode::Internal, error.to_string())
}

// ---- status of the latest run per chat (kept in memory) ----

fn statuses() -> &'static Mutex<HashMap<String, dto::HelperStatusDto>> {
    static STATUS: std::sync::OnceLock<Mutex<HashMap<String, dto::HelperStatusDto>>> =
        std::sync::OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn set_status(
    conversation_id: &str,
    job_id: &str,
    message_id: &str,
    state: dto::HelperRunState,
    error: Option<String>,
) {
    let mut map = statuses()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A newly queued read replaces the shown status; a late update of an
    // older job never hides the newer one.
    if state != dto::HelperRunState::Queued
        && map
            .get(conversation_id)
            .is_some_and(|current| current.job_id != job_id)
    {
        return;
    }
    map.insert(
        conversation_id.to_owned(),
        dto::HelperStatusDto {
            job_id: job_id.to_owned(),
            message_id: message_id.to_owned(),
            state,
            error,
        },
    );
}

fn status(conversation_id: &str) -> Option<dto::HelperStatusDto> {
    statuses()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(conversation_id)
        .cloned()
}

// ---- conversions ----

pub(crate) fn trackers_dto(trackers: Trackers) -> dto::HelperTrackersDto {
    dto::HelperTrackersDto {
        scene: trackers.scene,
        characters: trackers.characters,
        bonds: trackers.bonds,
        inventory: trackers.inventory,
        npcs: trackers.npcs,
        notebook: trackers.notebook,
    }
}

fn trackers_of(dto: dto::HelperTrackersDto) -> Trackers {
    Trackers {
        scene: dto.scene,
        characters: dto.characters,
        bonds: dto.bonds,
        inventory: dto.inventory,
        npcs: dto.npcs,
        notebook: dto.notebook,
    }
}

fn field_dto(field: &Field) -> dto::HelperFieldDto {
    dto::HelperFieldDto {
        name: field.name.clone(),
        value: field.value.clone(),
    }
}

fn entity_dto(entity: &Entity) -> dto::HelperEntityDto {
    dto::HelperEntityDto {
        name: entity.name.clone(),
        fields: entity.fields.iter().map(field_dto).collect(),
    }
}

pub(crate) fn state_dto(state: &HelperState) -> dto::HelperStateDto {
    dto::HelperStateDto {
        scene: state.scene.iter().map(field_dto).collect(),
        characters: state.characters.iter().map(entity_dto).collect(),
        bonds: state.bonds.iter().map(entity_dto).collect(),
        inventory: state
            .inventory
            .iter()
            .map(|holder| dto::HelperHolderDto {
                owner: holder.owner.clone(),
                items: holder.items.clone(),
            })
            .collect(),
        npcs: state.npcs.iter().map(entity_dto).collect(),
        notebook: state
            .notebook
            .iter()
            .map(|truth| dto::HelperTruthDto {
                id: truth.id.clone(),
                truth: truth.truth.clone(),
                hidden_from: truth.hidden_from.clone(),
                revealed: truth.revealed,
                clues: truth
                    .clues
                    .iter()
                    .map(|clue| dto::HelperClueDto {
                        text: clue.text.clone(),
                        message_id: clue.message_id.clone(),
                        found: clue.found,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn fields_of(fields: &[dto::HelperFieldDto]) -> Vec<Field> {
    fields
        .iter()
        .filter(|field| !field.name.trim().is_empty() && !field.value.trim().is_empty())
        .map(|field| Field {
            name: field.name.trim().to_owned(),
            value: field.value.trim().to_owned(),
        })
        .collect()
}

fn entities_of(list: &[dto::HelperEntityDto]) -> Vec<Entity> {
    list.iter()
        .filter(|entity| !entity.name.trim().is_empty())
        .map(|entity| Entity {
            name: entity.name.trim().to_owned(),
            fields: fields_of(&entity.fields),
        })
        .filter(|entity| !entity.fields.is_empty())
        .collect()
}

pub(crate) fn state_of(state: &dto::HelperStateDto) -> HelperState {
    HelperState {
        scene: fields_of(&state.scene),
        characters: entities_of(&state.characters),
        bonds: entities_of(&state.bonds),
        inventory: state
            .inventory
            .iter()
            .filter(|holder| !holder.owner.trim().is_empty())
            .map(|holder| Holder {
                owner: holder.owner.trim().to_owned(),
                items: holder
                    .items
                    .iter()
                    .map(|item| item.trim().to_owned())
                    .filter(|item| !item.is_empty())
                    .collect(),
            })
            .filter(|holder| !holder.items.is_empty())
            .collect(),
        npcs: entities_of(&state.npcs),
        notebook: state
            .notebook
            .iter()
            .filter(|truth| !truth.id.trim().is_empty() && !truth.truth.trim().is_empty())
            .map(|truth| Truth {
                id: truth.id.trim().to_owned(),
                truth: truth.truth.trim().to_owned(),
                hidden_from: truth.hidden_from.trim().to_owned(),
                revealed: truth.revealed,
                clues: truth
                    .clues
                    .iter()
                    .map(|clue| Clue {
                        text: clue.text.clone(),
                        message_id: clue.message_id.clone(),
                        found: clue.found,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn tracker_name(tracker: helper::Tracker) -> &'static str {
    match tracker {
        helper::Tracker::Scene => "scene",
        helper::Tracker::Characters => "characters",
        helper::Tracker::Bonds => "bonds",
        helper::Tracker::Inventory => "inventory",
        helper::Tracker::Npcs => "npcs",
        helper::Tracker::Notebook => "notebook",
    }
}

pub(crate) fn change_dto(op: &Op) -> dto::HelperChangeDto {
    let change = |kind: &str, subject: &str, field: &str, value: &str, found: Option<bool>| {
        dto::HelperChangeDto {
            tracker: tracker_name(op.tracker()).to_owned(),
            kind: kind.to_owned(),
            subject: subject.to_owned(),
            field: field.to_owned(),
            value: value.to_owned(),
            found,
        }
    };
    match op {
        Op::Set {
            entity,
            field,
            value,
            ..
        } => change("set", entity, field, value, None),
        Op::Remove { entity, .. } => change("remove", entity, "", "", None),
        Op::ItemAdd { owner, item } => change("item_add", owner, "", item, None),
        Op::ItemRemove { owner, item } => change("item_remove", owner, "", item, None),
        Op::Truth {
            id,
            truth,
            hidden_from,
        } => change("truth", id, hidden_from, truth, None),
        Op::Clue { id, text, found } => change("clue", id, "", text, *found),
        Op::Reveal { id, revealed } => {
            change("reveal", id, "", if *revealed { "1" } else { "0" }, None)
        }
        Op::TruthRemove { id } => change("truth_remove", id, "", "", None),
    }
}

// ---- reads ----

fn conversation_id(id: &str) -> Result<ConversationId, ApiError> {
    parse_id(id, "conversation_id")
}

struct ChatPath {
    conversation: lettuce_conversations::Conversation,
    items: Vec<lettuce_conversations::TimelineItem>,
    rows: Vec<lettuce_database::ForkHelperDeltaRow>,
}

fn chat_path(context: &ApiContext, id: ConversationId) -> Result<ChatPath, ApiError> {
    let database = context.backend().database();
    let conversation = ConversationReader::get(database, id)
        .map_err(IntoApiError::into_api_error)?
        .conversation;
    let items = crate::generation::fork_helper_run::shown_timeline(database, &conversation)
        .map_err(storage)?;
    let rows = database
        .fork_helper_deltas(&id.to_string())
        .map_err(storage)?;
    Ok(ChatPath {
        conversation,
        items,
        rows,
    })
}

impl ChatPath {
    fn state(&self) -> HelperState {
        let deltas = self
            .rows
            .iter()
            .map(|row| row.delta.clone())
            .collect::<Vec<_>>();
        helper::state_for_path(&deltas, &path_of(&self.items))
    }
}

fn chat_view(context: &ApiContext, id: ConversationId) -> Result<dto::HelperChatView, ApiError> {
    let database = context.backend().database();
    let path = chat_path(context, id)?;
    let key = id.to_string();
    let chat = database
        .fork_helper_scope(ForkHelperScopeKind::Conversation, &key)
        .map_err(storage)?;
    let (character_id, character_name) = match &path.conversation.kind {
        ConversationKind::Direct(details) => {
            let character_id = details.character.source_id;
            let name = lettuce_characters::CharacterRepository::get(database, character_id)
                .ok()
                .flatten()
                .map(|details| details.character.profile.name);
            (Some(character_id.to_string()), name)
        }
        ConversationKind::Group(_) => (None, None),
    };
    let character = character_id
        .as_deref()
        .map(|character| database.fork_helper_scope(ForkHelperScopeKind::Character, character))
        .transpose()
        .map_err(storage)?
        .flatten();
    let effective = database
        .fork_helper_effective(&key, &character_ids(&path.conversation))
        .map_err(storage)?;
    let trackers = effective
        .or(chat.map(|scope| scope.trackers))
        .or(character.map(|scope| scope.trackers))
        .unwrap_or_default();
    let scope_dto = |scope: ForkHelperScope| dto::HelperScopeDto {
        enabled: scope.enabled,
        trackers: trackers_dto(scope.trackers),
    };
    Ok(dto::HelperChatView {
        conversation_id: key.clone(),
        character_id,
        character_name,
        chat: chat.map(scope_dto),
        character: character.map(scope_dto),
        enabled: effective.is_some(),
        trackers: trackers_dto(trackers),
        state: state_dto(&path.state()),
        status: status(&key),
    })
}

pub async fn helper_chat_get(
    context: &ApiContext,
    request: dto::HelperChatRequest,
) -> Result<dto::HelperChatView, ApiError> {
    let id = conversation_id(&request.conversation_id)?;
    context
        .blocking(move |context| chat_view(context, id))
        .await
}

pub async fn helper_status(
    context: &ApiContext,
    request: dto::HelperChatRequest,
) -> Result<Option<dto::HelperStatusDto>, ApiError> {
    let id = conversation_id(&request.conversation_id)?;
    let _ = context;
    Ok(status(&id.to_string()))
}

pub async fn helper_scope_set(
    context: &ApiContext,
    request: dto::HelperScopeUpdate,
) -> Result<(), ApiError> {
    let kind = match request.scope.as_str() {
        "conversation" => {
            conversation_id(&request.id)?;
            ForkHelperScopeKind::Conversation
        }
        "character" => {
            parse_id::<lettuce_types::CharacterId>(&request.id, "id")?;
            ForkHelperScopeKind::Character
        }
        _ => {
            return Err(invalid_field(
                "scope",
                "scope must be conversation or character",
            ));
        }
    };
    context
        .blocking(move |context| {
            context
                .backend()
                .database()
                .fork_helper_scope_set(
                    kind,
                    &request.id,
                    ForkHelperScope {
                        enabled: request.enabled,
                        trackers: trackers_of(request.trackers),
                    },
                )
                .map_err(storage)
        })
        .await
}

fn config_view(config: ForkHelperConfig) -> dto::HelperConfigView {
    dto::HelperConfigView {
        model_profile_id: config.model_profile_id,
        prompt: config.prompt,
        default_prompt: helper::DEFAULT_PROMPT.to_owned(),
        history: config.history,
    }
}

pub async fn helper_config_get(context: &ApiContext) -> Result<dto::HelperConfigView, ApiError> {
    context
        .blocking(|context| {
            context
                .backend()
                .database()
                .fork_helper_config()
                .map(config_view)
                .map_err(storage)
        })
        .await
}

pub async fn helper_config_set(
    context: &ApiContext,
    request: dto::HelperConfigUpdate,
) -> Result<dto::HelperConfigView, ApiError> {
    if let Some(id) = request.model_profile_id.as_deref() {
        parse_id::<lettuce_types::ModelProfileId>(id, "model_profile_id")?;
    }
    if request
        .prompt
        .as_ref()
        .is_some_and(|p| p.len() > MAX_PROMPT_BYTES)
    {
        return Err(invalid_field("prompt", "the prompt is too long"));
    }
    if request.history > MAX_HELPER_HISTORY {
        return Err(invalid_field("history", "at most 40 messages"));
    }
    // The default prompt saved as is stays "default".
    let prompt = request.prompt.filter(|prompt| {
        !prompt.trim().is_empty() && prompt.trim() != helper::DEFAULT_PROMPT.trim()
    });
    context
        .blocking(move |context| {
            let database = context.backend().database();
            database
                .fork_helper_config_set(&ForkHelperConfig {
                    model_profile_id: request.model_profile_id,
                    prompt,
                    history: request.history,
                })
                .map_err(storage)?;
            database
                .fork_helper_config()
                .map(config_view)
                .map_err(storage)
        })
        .await
}

pub async fn helper_state_set(
    context: &ApiContext,
    request: dto::HelperStateUpdate,
) -> Result<dto::HelperChatView, ApiError> {
    let id = conversation_id(&request.conversation_id)?;
    let size = serde_json::to_vec(&request.state).map_or(usize::MAX, |bytes| bytes.len());
    if size > MAX_STATE_BYTES {
        return Err(invalid_field("state", "the state is too large"));
    }
    context
        .blocking(move |context| {
            let path = chat_path(context, id)?;
            let ops = helper::diff(&path.state(), &state_of(&request.state));
            if !ops.is_empty() {
                // Anchored to the latest reply: deleting or swiping it
                // rolls the edit back with the reply's own changes.
                let anchor = path
                    .items
                    .iter()
                    .rev()
                    .find(|item| item.message.role == MessageRole::Assistant);
                let (message_id, variant) = anchor
                    .map(|item| (item.message.id.to_string(), variant_key(item)))
                    .unwrap_or_default();
                context
                    .backend()
                    .database()
                    .fork_helper_add_edit(&id.to_string(), &message_id, &variant, &ops)
                    .map_err(storage)?;
            }
            chat_view(context, id)
        })
        .await
}

pub async fn helper_message(
    context: &ApiContext,
    request: dto::HelperMessageRequest,
) -> Result<dto::HelperMessageView, ApiError> {
    let id = conversation_id(&request.conversation_id)?;
    let message_id: MessageId = parse_id(&request.message_id, "message_id")?;
    context
        .blocking(move |context| {
            let path = chat_path(context, id)?;
            let empty = dto::HelperMessageView {
                read: false,
                changes: Vec::new(),
                raw: String::new(),
                edits: 0,
            };
            let Some(item) = path.items.iter().find(|item| item.message.id == message_id) else {
                return Ok(empty);
            };
            let key = message_id.to_string();
            let variant = variant_key(item);
            let rows = path
                .rows
                .iter()
                .filter(|row| row.delta.message_id == key && row.delta.variant_key == variant)
                .collect::<Vec<_>>();
            let read = rows
                .iter()
                .rev()
                .find(|row| row.delta.source == DeltaSource::Helper);
            let edits = rows
                .iter()
                .filter(|row| row.delta.source == DeltaSource::Manual)
                .count();
            Ok(dto::HelperMessageView {
                read: read.is_some(),
                changes: read
                    .map(|row| row.delta.ops.iter().map(change_dto).collect())
                    .unwrap_or_default(),
                raw: read.map(|row| row.raw.clone()).unwrap_or_default(),
                edits: u32::try_from(edits).unwrap_or(u32::MAX),
            })
        })
        .await
}

/// After a completed reply: queues a helper read of the chat's latest reply
/// when the helper is on. Never fails the turn.
pub(crate) fn start_auto(context: &ApiContext, conversation_id: ConversationId) {
    if let Err(error) = super::jobs::fork_helper::admit_latest(context, conversation_id) {
        tracing::warn!(?error, %conversation_id, "the helper read could not be queued");
    }
}

#[cfg(test)]
#[path = "fork_helper_tests.rs"]
mod tests;
