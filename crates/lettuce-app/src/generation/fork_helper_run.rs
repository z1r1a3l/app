//! Fork-only: one helper model read of one reply.
//!
//! The helper reads the state as it stood before the reply, a few earlier
//! messages and the latest exchange, and answers with change lines. Only
//! lines of enabled trackers are kept, the Notebook keeps at most three
//! unrevealed truths, and each claimed clue is checked against the reply.
//! The read is stored under the reply variant it was made for, so a swipe,
//! edit or delete leaves it out of the fold.

use lettuce_characters::{CharacterRepository, PersonaRepository};
use lettuce_context::fork_helper::{
    self as helper, HelperInput, HelperLine, MAX_LIVE_TRUTHS, Op, Trackers,
};
use lettuce_conversations::{
    Conversation, ConversationKind, ConversationReader, GenerationOperation, InferenceRequest,
    MessageRole, MessageVisibility, OutputPolicy, ParticipantSource, PortError,
    ProviderContextPart, ProviderFailureKind, ProviderNeutralContext, ProviderNeutralMessage,
    ResolvedInferenceProfile, SafetyContext, TimelineItem, ToolPolicy, effective_persona,
};
use lettuce_database::Database;
use lettuce_jobs::{JobError, JobErrorCode, handle::JobHandle};
use lettuce_models::{
    ChatRequirements, ExpectedModelIdentity, ModelProfileRepository, ProviderAccountRepository,
};
use lettuce_settings::GlobalSettingsStore;
use lettuce_types::{
    ConversationId, GenerationAttemptId, GenerationTurnId, MessageId, PageLimit, PageRequest,
    RequestId, TimestampMillis,
};

use crate::generation::fork_prompt_helper::{character_ids, path_of, variant_key};
use crate::jobs::job_inference_usage::{JobInferenceError, run_job_inference};

pub(crate) const HELPER_DEFAULTS: crate::FeatureSamplingDefaults =
    crate::FeatureSamplingDefaults::with_max_tokens(0.3, 1200);
const MAX_LINE_CHARS: usize = 6000;

/// One read: the reply variant it is for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct HelperRunRequest {
    pub request_id: RequestId,
    pub conversation_id: ConversationId,
    pub message_id: MessageId,
    pub variant_key: String,
}

/// What a finished read did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HelperRunOutcome {
    Stored { ops: Vec<Op>, skipped: usize },
    /// The reply, its variant or the switch changed meanwhile; nothing kept.
    Dropped,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum HelperRunError {
    #[error("helper storage failed: {0}")]
    Storage(String),
    #[error("No model is set for the helper")]
    MissingModel,
    #[error("The helper model is invalid: {0}")]
    InvalidModel(String),
    #[error("The helper request is invalid: {0}")]
    InvalidRequest(String),
    #[error("The helper model failed: {0}")]
    Inference(PortError),
    #[error("The helper model returned nothing")]
    EmptyResponse,
    #[error("The helper read was stopped")]
    Cancelled,
    #[error("helper job storage failed: {0:?}")]
    Jobs(lettuce_jobs::StoreError),
    #[error("helper job was already settled")]
    AlreadySettled,
    #[error("helper job could not be claimed")]
    NotClaimed,
}

impl From<crate::jobs::one_shot_job::OneShotJobError> for HelperRunError {
    fn from(error: crate::jobs::one_shot_job::OneShotJobError) -> Self {
        match error {
            crate::jobs::one_shot_job::OneShotJobError::Jobs(error) => Self::Jobs(error),
            crate::jobs::one_shot_job::OneShotJobError::AlreadySettled => Self::AlreadySettled,
            crate::jobs::one_shot_job::OneShotJobError::NotClaimed => Self::NotClaimed,
        }
    }
}

impl crate::jobs::one_shot_job::OneShotFailure for HelperRunError {
    fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    fn job_error(&self) -> JobError {
        let (code, retryable, message) = match self {
            Self::Inference(PortError::Provider(failure))
                if failure.kind == ProviderFailureKind::Unavailable =>
            {
                (JobErrorCode::ResourceUnavailable, true, "fork-helper-unavailable")
            }
            Self::Inference(PortError::Unavailable) | Self::NotClaimed | Self::AlreadySettled => {
                (JobErrorCode::ResourceUnavailable, true, "fork-helper-unavailable")
            }
            Self::Inference(_) => (JobErrorCode::WorkerFailed, false, "fork-helper-inference-failed"),
            Self::EmptyResponse => (JobErrorCode::WorkerFailed, false, "fork-helper-no-answer"),
            Self::Storage(_) | Self::Jobs(_) => {
                (JobErrorCode::StorageFailure, true, "fork-helper-storage-failed")
            }
            Self::MissingModel => (JobErrorCode::InvalidInput, false, "fork-helper-no-model"),
            Self::InvalidModel(_) | Self::InvalidRequest(_) | Self::Cancelled => {
                (JobErrorCode::InvalidInput, false, "fork-helper-invalid-input")
            }
        };
        JobError::new(code, retryable, message).expect("constant job error is valid")
    }
}

fn storage(error: impl std::fmt::Display) -> HelperRunError {
    HelperRunError::Storage(error.to_string())
}

/// Every message of the chat's shown branch, oldest first.
pub(crate) fn shown_timeline(
    database: &Database,
    conversation: &Conversation,
) -> Result<Vec<TimelineItem>, HelperRunError> {
    let mut items = Vec::new();
    let mut cursor = None;
    loop {
        let page = database
            .timeline_page(
                conversation.id,
                conversation.active_branch_id,
                &PageRequest {
                    cursor,
                    limit: PageLimit::new(200),
                },
            )
            .map_err(storage)?;
        items.extend(page.items);
        let Some(next) = page.next_cursor else {
            break;
        };
        cursor = Some(next);
    }
    items.reverse();
    Ok(items)
}

/// The shown variant's text parts (as the chat shows them).
pub(crate) fn item_text(item: &TimelineItem) -> String {
    let parts = match item.message.active_render_source {
        lettuce_conversations::MessageRenderSource::Revision(_) => {
            item.active_revision.as_ref().map(|value| &value.parts)
        }
        lettuce_conversations::MessageRenderSource::Candidate(_) => {
            item.active_candidate.as_ref().map(|value| &value.parts)
        }
    };
    parts
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|part| match part {
            lettuce_conversations::MessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn shorten(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_LINE_CHARS {
        return text.to_owned();
    }
    let mut out = text.chars().take(MAX_LINE_CHARS).collect::<String>();
    out.push('…');
    out
}

struct Names {
    user: String,
    character: String,
    card: String,
    speakers: Vec<(lettuce_types::ConversationParticipantId, String)>,
}

fn names(database: &Database, conversation: &Conversation) -> Result<Names, HelperRunError> {
    let user = effective_persona(conversation)
        .map(|persona| PersonaRepository::get(database, persona.source_id))
        .transpose()
        .map_err(|error| storage(format!("{error:?}")))?
        .flatten()
        .map_or_else(|| "User".to_owned(), |persona| persona.title);
    let mut speakers = Vec::new();
    let mut members = Vec::new();
    let mut card = String::new();
    for participant in &conversation.participants {
        let ParticipantSource::Character(character_id) = participant.source else {
            continue;
        };
        if let Some(details) = CharacterRepository::get(database, character_id)
            .map_err(|error| storage(format!("{error:?}")))?
        {
            let profile = details.character.profile;
            speakers.push((participant.id, profile.name.clone()));
            if matches!(conversation.kind, ConversationKind::Direct(_)) || card.is_empty() {
                card = profile
                    .definition
                    .as_deref()
                    .or(profile.description.as_deref())
                    .unwrap_or_default()
                    .to_owned();
            }
            members.push(profile.name);
        }
    }
    if matches!(conversation.kind, ConversationKind::Group(_)) {
        card.clear();
    }
    Ok(Names {
        user,
        character: if members.is_empty() {
            "Character".to_owned()
        } else {
            members.join(", ")
        },
        card,
        speakers,
    })
}

fn line(item: &TimelineItem, names: &Names) -> Option<HelperLine> {
    if item.message.visibility != MessageVisibility::Visible {
        return None;
    }
    let speaker = match item.message.role {
        MessageRole::User => names.user.clone(),
        MessageRole::Assistant => item
            .message
            .author_participant_id
            .and_then(|author| {
                names
                    .speakers
                    .iter()
                    .find(|(id, _)| *id == author)
                    .map(|(_, name)| name.clone())
            })
            .unwrap_or_else(|| names.character.clone()),
        _ => return None,
    };
    let text = item_text(item);
    (!text.trim().is_empty()).then(|| HelperLine {
        speaker,
        text: shorten(&text),
    })
}

/// The ops kept from an answer: enabled trackers only, at most three live
/// truths, and each clue checked against the reply.
pub(crate) fn keep_ops(
    answer: &str,
    trackers: &Trackers,
    before: &helper::HelperState,
    reply: &str,
) -> (Vec<Op>, usize) {
    let parsed = helper::parse_delta(answer);
    let ops = parsed
        .ops
        .into_iter()
        .filter(|op| trackers.enabled(op.tracker()))
        .collect::<Vec<_>>();
    let ops = helper::limit_new_truths(ops, before, MAX_LIVE_TRUTHS)
        .into_iter()
        .map(|op| match op {
            Op::Clue { id, text, .. } => {
                let found = Some(helper::clue_found(&text, reply));
                Op::Clue { id, text, found }
            }
            other => other,
        })
        .collect();
    (ops, parsed.skipped.len())
}

/// Runs one read with `handle`'s job and stores it.
pub(crate) async fn run_helper<I>(
    database: &Database,
    inference: &I,
    request: &HelperRunRequest,
    handle: &JobHandle,
    now: TimestampMillis,
) -> Result<HelperRunOutcome, HelperRunError>
where
    I: lettuce_conversations::InferencePort + ?Sized,
{
    let aggregate = ConversationReader::get(database, request.conversation_id).map_err(storage)?;
    let conversation = &aggregate.conversation;
    let conversation_key = conversation.id.to_string();
    let Some(trackers) = database
        .fork_helper_effective(&conversation_key, &character_ids(conversation))
        .map_err(storage)?
    else {
        return Ok(HelperRunOutcome::Dropped);
    };
    let items = shown_timeline(database, conversation)?;
    let Some(index) = items
        .iter()
        .position(|item| item.message.id == request.message_id)
    else {
        return Ok(HelperRunOutcome::Dropped);
    };
    let reply = &items[index];
    if variant_key(reply) != request.variant_key || reply.message.role != MessageRole::Assistant {
        return Ok(HelperRunOutcome::Dropped);
    }
    let rows = database
        .fork_helper_deltas(&conversation_key)
        .map_err(storage)?
        .into_iter()
        .map(|row| row.delta)
        .collect::<Vec<_>>();
    let before = helper::state_for_path(&rows, &path_of(&items[..index]));
    let config = database.fork_helper_config().map_err(storage)?;
    let names = names(database, conversation)?;

    // The latest exchange: the user's message just before the reply, if any.
    let mut start = index;
    if index > 0 && items[index - 1].message.role == MessageRole::User {
        start = index - 1;
    }
    let latest = items[start..=index]
        .iter()
        .filter_map(|item| line(item, &names))
        .collect::<Vec<_>>();
    let mut earlier = items[..start]
        .iter()
        .rev()
        .filter_map(|item| line(item, &names))
        .take(config.history as usize)
        .collect::<Vec<_>>();
    earlier.reverse();
    let input = HelperInput {
        character_name: names.character.clone(),
        character_card: names.card.clone(),
        user_name: names.user.clone(),
        state: helper::render_state_block(&before, &trackers, ""),
        earlier,
        latest,
        trackers,
        live_truths: helper::live_truths(&before),
    };
    let system = helper::helper_system_prompt(
        config.prompt.as_deref().unwrap_or_default(),
        &names.user,
        &names.character,
    );

    let stored = GlobalSettingsStore::load(database).map_err(|e| storage(format!("{e:?}")))?;
    let model_id = config
        .model_profile_id
        .as_deref()
        .and_then(|id| id.parse::<lettuce_types::ModelProfileId>().ok())
        .or(stored.default_model_profile_id)
        .ok_or(HelperRunError::MissingModel)?;
    let model = ModelProfileRepository::get(database, model_id)
        .map_err(|error| HelperRunError::InvalidModel(error.to_string()))?
        .ok_or(HelperRunError::MissingModel)?;
    let account = ProviderAccountRepository::get(database, model.provider_account_id)
        .map_err(|error| HelperRunError::InvalidModel(error.to_string()))?
        .ok_or(HelperRunError::MissingModel)?;
    let global = lettuce_models::GlobalModelSettingsRepository::global_model_settings(database)
        .map_err(|error| HelperRunError::InvalidModel(error.to_string()))?
        .0;
    let chat_profile = lettuce_models::resolve_chat_profile(
        &ExpectedModelIdentity {
            model_profile_id: model.id,
            model_revision: model.revision,
            provider_account_id: account.id,
            provider_account_revision: account.revision,
            external_model_id: model.external_model_id.clone(),
            display_name: model.display_name.clone(),
            provider_protocol: account.protocol,
            model_kind: model.kind,
        },
        &model,
        &account,
        &crate::feature_parameter_input(
            &lettuce_models::FeatureGenerationParameters::default(),
            HELPER_DEFAULTS,
            crate::FeatureRequestFields::Sampling,
            account.protocol,
            &global,
        ),
        &ChatRequirements::default(),
    )
    .map_err(|error| HelperRunError::InvalidModel(error.to_string()))?;
    let inference_request = InferenceRequest {
        turn_id: GenerationTurnId::from_uuid(request.request_id.as_uuid()),
        attempt_id: GenerationAttemptId::from_uuid(uuid::Uuid::new_v5(
            &handle.id().as_uuid(),
            b"fork-helper",
        )),
        operation: GenerationOperation::Send,
        profile: ResolvedInferenceProfile {
            chat_profile,
            tool_policy: ToolPolicy::Disabled,
            output_policy: OutputPolicy::Plain,
            safety_policy: SafetyContext::Standard,
            correlation_id: None,
        },
        context: ProviderNeutralContext {
            messages: vec![
                ProviderNeutralMessage {
                    role: MessageRole::System,
                    parts: vec![ProviderContextPart::Text { text: system }],
                },
                ProviderNeutralMessage {
                    role: MessageRole::User,
                    parts: vec![ProviderContextPart::Text {
                        text: helper::helper_user_message(&input),
                    }],
                },
            ],
            attributions: Default::default(),
            budget: Default::default(),
        },
        cancellation: Some(handle.id()),
        stream_sink: None,
        media_grants: Vec::new(),
        tools: None,
        prompt_cache_key: None,
    };
    inference_request
        .validate()
        .map_err(|error| HelperRunError::InvalidRequest(error.to_string()))?;
    if handle.cancellation_token().is_cancelled() {
        return Err(HelperRunError::Cancelled);
    }
    let outcome = match run_job_inference(database, inference, handle.id(), inference_request, now).await {
        Ok(outcome) => outcome,
        Err(JobInferenceError::Provider(PortError::Cancelled)) => {
            return Err(HelperRunError::Cancelled);
        }
        Err(JobInferenceError::Provider(error)) => return Err(HelperRunError::Inference(error)),
        Err(JobInferenceError::Evidence { reason }) => return Err(storage(reason)),
    };
    let answer = outcome
        .candidates
        .first()
        .map(|candidate| {
            candidate
                .parts
                .iter()
                .filter_map(|part| match part {
                    lettuce_conversations::MessagePart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    crate::cleanup_outcome_replays(database, &outcome).map_err(storage)?;
    if handle.cancellation_token().is_cancelled() {
        return Err(HelperRunError::Cancelled);
    }
    if answer.trim().is_empty() {
        return Err(HelperRunError::EmptyResponse);
    }
    let (ops, skipped) = keep_ops(&answer, &trackers, &before, &item_text(reply));
    database
        .fork_helper_put_read(
            &conversation_key,
            &request.message_id.to_string(),
            &request.variant_key,
            &ops,
            answer.trim(),
        )
        .map_err(storage)?;
    Ok(HelperRunOutcome::Stored { ops, skipped })
}
