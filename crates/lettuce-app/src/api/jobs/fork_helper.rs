//! Fork-only: the helper model's background read of a finished reply. The
//! post-reply hook admits one job per reply variant; jobs of one chat run in
//! order (one lane per chat). Stopping goes through `jobCancel`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use lettuce_contracts::{self as dto, ApiError};
use lettuce_conversations::{ConversationReader, MessageRole};
use lettuce_jobs::{
    JobKind, JobSnapshot, ResourceAvailability, WorkerId, handle::CancellationToken,
};
use lettuce_types::{ConversationId, JobId, RequestId};

use super::local::{digest, encode, internal};
use super::runner::{ClaimedJob, JobHandler, JobLane, JobProgressSink};
use crate::api::ApiContext;
use crate::generation::fork_helper_run::{
    HelperRunError, HelperRunOutcome, HelperRunRequest, run_helper, shown_timeline,
};
use crate::generation::fork_prompt_helper::{character_ids, variant_key};

const HELPER_LEASE: Duration = Duration::from_secs(30 * 60);
const JOB_NAME: &str = "fork-helper";
const PREFIX: &str = "fork-helper-";

fn is_helper(job: &JobSnapshot) -> bool {
    job.idempotency_key
        .as_ref()
        .is_some_and(|key| key.as_str().starts_with(PREFIX))
}

/// Queues a read of the chat's latest reply (its shown variant) when the
/// helper is on for the chat; returns the job, or `None` when it is off or
/// the latest message is not a reply. Runs on a blocking thread.
pub(crate) fn admit_latest(
    context: &ApiContext,
    conversation_id: ConversationId,
) -> Result<Option<JobId>, ApiError> {
    let database = context.backend().database();
    let conversation = ConversationReader::get(database, conversation_id)
        .map_err(internal)?
        .conversation;
    let key = conversation_id.to_string();
    if database
        .fork_helper_effective(&key, &character_ids(&conversation))
        .map_err(internal)?
        .is_none()
    {
        return Ok(None);
    }
    let items = shown_timeline(database, &conversation).map_err(internal)?;
    let Some(reply) = items.last() else {
        return Ok(None);
    };
    if reply.message.role != MessageRole::Assistant {
        return Ok(None);
    }
    let request_id = RequestId::new();
    let request = HelperRunRequest {
        request_id,
        conversation_id,
        message_id: reply.message.id,
        variant_key: variant_key(reply),
    };
    let operation = format!("fork_helper:{request_id}");
    let request_digest = digest(&request)?;
    let job = database
        .admit_job_with_detail(
            crate::jobs::one_shot_job::one_shot_spec(crate::jobs::one_shot_job::OneShotJob {
                name: JOB_NAME,
                stage: JOB_NAME,
                subject_kind: lettuce_jobs::SubjectKind::Conversation,
                subject: &key,
                request_id,
            }),
            &operation,
            &request_digest,
            &encode(&request)?,
        )
        .map_err(internal)?;
    crate::api::fork_helper::set_status(
        &key,
        &job.id.to_string(),
        &reply.message.id.to_string(),
        dto::HelperRunState::Queued,
        None,
    );
    context.jobs().wake();
    Ok(Some(job.id))
}

/// Runs the helper reads the post-reply hook admitted.
#[derive(Debug, Clone, Copy, Default)]
pub struct ForkHelperHandler;

#[async_trait]
impl JobHandler for ForkHelperHandler {
    fn kinds(&self) -> &[JobKind] {
        &[JobKind::CreationRun]
    }

    fn lane(&self, _context: &ApiContext, job: &JobSnapshot) -> Option<JobLane> {
        is_helper(job).then(|| JobLane(format!("fork-helper:{}", job.subject.id.as_str())))
    }

    async fn claim(
        &self,
        context: &ApiContext,
        job: &JobSnapshot,
        worker_id: WorkerId,
    ) -> Result<Option<Box<dyn ClaimedJob>>, ApiError> {
        let job_id = job.id;
        let claimed = context
            .blocking(move |context| {
                use lettuce_jobs::{JobMutation, JobStore};
                let database = context.backend().database();
                let record = database.job_detail(job_id).map_err(internal)?;
                let request = record.and_then(|record| {
                    serde_json::from_value::<HelperRunRequest>(record.detail).ok()
                });
                let Some(claim) = database
                    .claim(
                        job_id,
                        worker_id,
                        context.now(),
                        HELPER_LEASE,
                        &ResourceAvailability::all(),
                    )
                    .map_err(internal)?
                else {
                    return Ok(None);
                };
                database
                    .append_and_transition(JobMutation::Start {
                        claim: claim.claim.clone(),
                        at: context.now(),
                    })
                    .map_err(internal)?;
                Ok(Some((claim.claim, request)))
            })
            .await?;
        let Some((claim, request)) = claimed else {
            return Ok(None);
        };
        let Some(request) = request else {
            // An unreadable detail: settle the job as invalid.
            settle_failed(
                context,
                claim,
                &HelperRunError::InvalidRequest("the helper job detail is unreadable".to_owned()),
            )
            .await?;
            return Ok(None);
        };
        Ok(Some(Box::new(ClaimedHelper {
            claim,
            request,
            cancellation: CancellationToken::new(),
        })))
    }
}

struct ClaimedHelper {
    claim: lettuce_jobs::ClaimRef,
    request: HelperRunRequest,
    cancellation: CancellationToken,
}

#[async_trait]
impl ClaimedJob for ClaimedHelper {
    fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    async fn run(
        self: Box<Self>,
        context: ApiContext,
        _progress: Arc<dyn JobProgressSink>,
    ) -> Result<(), ApiError> {
        let conversation = self.request.conversation_id.to_string();
        let job_id = self.claim.job_id.to_string();
        let message_id = self.request.message_id.to_string();
        crate::api::fork_helper::set_status(
            &conversation,
            &job_id,
            &message_id,
            dto::HelperRunState::Running,
            None,
        );
        let backend = context.backend();
        let runtime = backend.inference_runtime();
        let allowed = ResourceAvailability::all();
        let now = context.now();
        let database = backend.database();
        let request = &self.request;
        let inference = context.inference();
        let outcome = crate::jobs::one_shot_job::run_one_shot_job(
            database,
            crate::jobs::one_shot_job::OneShotJob {
                name: JOB_NAME,
                stage: JOB_NAME,
                subject_kind: lettuce_jobs::SubjectKind::Conversation,
                subject: &conversation,
                request_id: request.request_id,
            },
            crate::jobs::one_shot_job::OneShotLease {
                claim: Some(&self.claim),
                worker_id: WorkerId::new(),
                now,
                lease_for: HELPER_LEASE,
                allowed: &allowed,
                cancellations: Some(runtime.as_ref()),
                cancellation: Some(&self.cancellation),
            },
            |handle| async move { run_helper(database, inference, request, &handle, now).await },
        )
        .await;
        let (state, error) = match &outcome {
            Ok((HelperRunOutcome::Stored { .. }, _)) => (dto::HelperRunState::Done, None),
            Ok((HelperRunOutcome::Dropped, _)) => (dto::HelperRunState::Cancelled, None),
            Err(HelperRunError::Cancelled) => (dto::HelperRunState::Cancelled, None),
            Err(error) => (dto::HelperRunState::Failed, Some(error.to_string())),
        };
        crate::api::fork_helper::set_status(&conversation, &job_id, &message_id, state, error);
        if let Err(error) = outcome {
            tracing::warn!(job_id = %self.claim.job_id, %error, "the helper read failed");
        }
        Ok(())
    }
}

async fn settle_failed(
    context: &ApiContext,
    claim: lettuce_jobs::ClaimRef,
    error: &HelperRunError,
) -> Result<(), ApiError> {
    let error = crate::jobs::one_shot_job::OneShotFailure::job_error(error);
    context
        .blocking(move |context| {
            use lettuce_jobs::{JobMutation, JobStore};
            context
                .backend()
                .database()
                .append_and_transition(JobMutation::Fail {
                    claim,
                    error,
                    at: context.now(),
                })
                .map_err(internal)?;
            Ok(())
        })
        .await
}
