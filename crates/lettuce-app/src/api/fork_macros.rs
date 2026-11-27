//! Fork-only: macro variable API (per-chat inspector, globals), the prompt
//! preview's macro pass, and the dice a reply's prompt rolled.

use lettuce_context::fork_macros::{
    self as engine, DiceRoll, MAX_VALUE_BYTES, MacroEnv, MacroRun, Vars,
};
use lettuce_contracts::{self as dto, ApiError, ApiErrorCode};

use super::ApiContext;
use super::error::{IntoApiError, api_error, invalid_field, parse_id};

const MAX_VARS: usize = 5000;
const MAX_NAME_BYTES: usize = 512;
const MAX_PREVIEW_TEXTS: usize = 2000;

fn storage(error: impl std::fmt::Display) -> ApiError {
    api_error(ApiErrorCode::Internal, error.to_string())
}

fn list(vars: Vars) -> Vec<dto::MacroVarDto> {
    vars.into_iter()
        .map(|(name, value)| dto::MacroVarDto { name, value })
        .collect()
}

pub(crate) fn roll_dto(roll: &DiceRoll) -> dto::DiceRollDto {
    dto::DiceRollDto {
        formula: roll.formula.clone(),
        dice: roll
            .dice
            .iter()
            .map(|face| u32::try_from(*face).unwrap_or(u32::MAX))
            .collect(),
        modifier: roll.modifier,
        total: roll.total,
    }
}

/// A chat's variables (none for an empty id) and the globals.
pub(crate) fn vars_dto(
    database: &lettuce_database::Database,
    conversation_id: &str,
) -> Result<(Vec<dto::MacroVarDto>, Vec<dto::MacroVarDto>), ApiError> {
    let chat = if conversation_id.is_empty() {
        Vars::new()
    } else {
        database.fork_chat_vars(conversation_id).map_err(storage)?
    };
    let global = database.fork_global_vars().map_err(storage)?;
    Ok((list(chat), list(global)))
}

fn conversation(id: Option<&str>) -> Result<Option<String>, ApiError> {
    id.map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| {
            parse_id::<lettuce_types::ConversationId>(id, "conversation_id")
                .map(|id| id.to_string())
        })
        .transpose()
}

fn view(
    database: &lettuce_database::Database,
    conversation_id: Option<String>,
) -> Result<dto::MacroVarsView, ApiError> {
    let (chat, global) = vars_dto(database, conversation_id.as_deref().unwrap_or_default())?;
    Ok(dto::MacroVarsView {
        conversation_id,
        chat,
        global,
    })
}

pub async fn macro_vars_get(
    context: &ApiContext,
    request: dto::MacroVarsRequest,
) -> Result<dto::MacroVarsView, ApiError> {
    let conversation_id = conversation(request.conversation_id.as_deref())?;
    context
        .blocking(move |context| view(context.backend().database(), conversation_id))
        .await
}

fn checked(vars: Vec<dto::MacroVarDto>) -> Result<Vars, ApiError> {
    if vars.len() > MAX_VARS {
        return Err(invalid_field("vars", "too many variables"));
    }
    let mut out = Vars::new();
    for var in vars {
        let name = var.name.trim().to_owned();
        if name.is_empty() {
            return Err(invalid_field("vars.name", "a variable needs a name"));
        }
        if name.len() > MAX_NAME_BYTES {
            return Err(invalid_field("vars.name", "the name is too long"));
        }
        if var.value.len() > MAX_VALUE_BYTES {
            return Err(invalid_field("vars.value", "the value is too long"));
        }
        if out.insert(name, var.value).is_some() {
            return Err(invalid_field("vars.name", "two variables share a name"));
        }
    }
    Ok(out)
}

pub async fn macro_vars_replace(
    context: &ApiContext,
    request: dto::MacroVarsReplaceRequest,
) -> Result<dto::MacroVarsView, ApiError> {
    let conversation_id = conversation(request.conversation_id.as_deref())?;
    if !request.global && conversation_id.is_none() {
        return Err(invalid_field(
            "conversation_id",
            "chat variables need a chat",
        ));
    }
    let vars = checked(request.vars)?;
    let global = request.global;
    context
        .blocking(move |context| {
            let database = context.backend().database();
            database
                .fork_vars_replace(
                    if global {
                        None
                    } else {
                        conversation_id.as_deref()
                    },
                    &vars,
                )
                .map_err(storage)?;
            view(database, conversation_id)
        })
        .await
}

pub async fn macro_preview(
    context: &ApiContext,
    request: dto::MacroPreviewRequest,
) -> Result<dto::MacroPreviewResult, ApiError> {
    let conversation_id = conversation(request.conversation_id.as_deref())?;
    if request.texts.len() > MAX_PREVIEW_TEXTS {
        return Err(invalid_field("texts", "too many texts"));
    }
    context
        .blocking(move |context| {
            let database = context.backend().database();
            let chat = match &conversation_id {
                Some(id) => database.fork_chat_vars(id).map_err(storage)?,
                None => Vars::new(),
            };
            let globals = database.fork_global_vars().map_err(storage)?;
            let mut run = MacroRun::new(
                MacroEnv {
                    user: request.user_name,
                    char: request.character_name,
                    pick_seed: conversation_id.clone().unwrap_or_default(),
                    custom: conversation_id
                        .as_deref()
                        .map(|id| super::fork_chat_regex::chat_macros(database, id))
                        .unwrap_or_default(),
                    ..MacroEnv::default()
                },
                chat,
                globals,
            );
            let texts = request
                .texts
                .iter()
                .map(|text| run.evaluate(text).trim().to_owned())
                .collect();
            let changed = run
                .chat_changes()
                .keys()
                .chain(run.global_changes().keys())
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            Ok(dto::MacroPreviewResult {
                texts,
                rolls: run.rolls().iter().map(roll_dto).collect(),
                vars: dto::MacroVarsView {
                    conversation_id,
                    chat: list(run.vars().clone()),
                    global: list(run.globals().clone()),
                },
                changed,
            })
        })
        .await
}

/// The dice of the generation that produced a reply, found by the
/// fingerprint of its recorded prompt.
pub async fn macro_message_rolls(
    context: &ApiContext,
    request: dto::MacroRollsRequest,
) -> Result<dto::MacroRollsView, ApiError> {
    let message_id = request.message_id.clone();
    let snapshot = match super::message_prompt_snapshot(
        context,
        dto::MessagePromptSnapshotRequest { message_id },
    )
    .await
    {
        Ok(snapshot) => snapshot,
        Err(error) if error.code == ApiErrorCode::NotFound => {
            return Ok(dto::MacroRollsView { rolls: Vec::new() });
        }
        Err(error) => return Err(error),
    };
    let fingerprint = engine::context_fingerprint(snapshot.messages.iter().flat_map(|message| {
        message.parts.iter().filter_map(|part| match part {
            dto::PromptPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }));
    let message_id: lettuce_types::MessageId = parse_id(&request.message_id, "message_id")?;
    context
        .blocking(move |context| {
            let database = context.backend().database();
            let conversation_id =
                lettuce_conversations::ConversationOverviewReader::conversation_of_message(
                    database, message_id,
                )
                .map_err(IntoApiError::into_api_error)?
                .ok_or_else(|| api_error(ApiErrorCode::NotFound, "the message was not found"))?;
            let rolls = database
                .fork_rolls_for(&conversation_id.to_string(), &fingerprint)
                .map_err(storage)?
                .unwrap_or_default();
            Ok(dto::MacroRollsView {
                rolls: rolls.iter().map(roll_dto).collect(),
            })
        })
        .await
}
