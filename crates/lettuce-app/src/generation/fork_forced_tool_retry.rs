//! Fork (z1r1a3l): provider-agnostic retry for requests that force a tool.
//!
//! Many models think by default and their APIs reject a forced `tool_choice`
//! ("required" or a named function) with HTTP 400 (DeepSeek V4: "Thinking
//! mode does not support this tool_choice"). When that happens the request is
//! sent once more with `tool_choice: auto` and an explicit instruction to call
//! the tool. A reply without a matching tool call is reported as an invalid
//! response, so callers take their structured fallback as before.

use lettuce_conversations::{
    InferenceOutcome, InferencePort, InferenceRequest, MessageRole, PortError, ProviderContextPart,
    ProviderFailure, ProviderNeutralMessage, ToolChoice, ToolPolicy,
};

const HINTS: [&str; 8] = [
    "tool_choice",
    "tool choice",
    "toolchoice",
    "tool_config",
    "function_call",
    "thinking",
    "reasoning",
    "forced",
];

/// Whether a provider failure looks like a rejection of the forced tool choice.
pub(crate) fn is_tool_choice_rejection(failure: &ProviderFailure) -> bool {
    if !matches!(failure.status, 400 | 422) {
        return false;
    }
    let text = format!(
        "{} {}",
        failure.message.as_deref().unwrap_or_default(),
        failure.code.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    HINTS.iter().any(|hint| text.contains(hint))
}

fn relaxed(mut request: InferenceRequest) -> Option<InferenceRequest> {
    let tools = request.tools.as_mut()?;
    let names = tools
        .definitions
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    let instruction = match &tools.choice {
        ToolChoice::Auto => return None,
        ToolChoice::Named { name } => format!(
            "You MUST answer by calling the `{name}` tool now. Do not reply with plain text."
        ),
        ToolChoice::Required => format!(
            "You MUST answer by calling exactly one of these tools now: {}. Do not reply with plain text.",
            names.join(", ")
        ),
    };
    tools.choice = ToolChoice::Auto;
    request.profile.tool_policy = ToolPolicy::Allowed;
    request.context.messages.push(ProviderNeutralMessage {
        role: MessageRole::User,
        parts: vec![ProviderContextPart::Text { text: instruction }],
    });
    Some(request)
}

/// Runs `request`; on a forced-tool rejection retries once with `auto`.
pub(crate) async fn run<P: InferencePort + ?Sized>(
    inner: &P,
    request: InferenceRequest,
) -> Result<InferenceOutcome, PortError> {
    let forced = request
        .tools
        .as_ref()
        .and_then(|tools| match &tools.choice {
            ToolChoice::Auto => None,
            ToolChoice::Required => Some(None),
            ToolChoice::Named { name } => Some(Some(name.clone())),
        });
    let Some(named) = forced else {
        return inner.run(request).await;
    };
    let retry = relaxed(request.clone());
    let failure = match inner.run(request).await {
        Err(PortError::Provider(failure)) if is_tool_choice_rejection(&failure) => failure,
        other => return other,
    };
    let Some(retry) = retry else {
        return Err(PortError::Provider(failure));
    };
    tracing::warn!(
        status = failure.status,
        code = ?failure.code,
        "provider rejected the forced tool_choice; retrying once with tool_choice auto"
    );
    let outcome = inner.run(retry).await?;
    let called = outcome
        .candidates
        .iter()
        .flat_map(|candidate| &candidate.tool_calls)
        .any(|call| named.as_ref().is_none_or(|name| *name == call.name));
    if called {
        Ok(outcome)
    } else {
        tracing::warn!("auto tool_choice retry returned no tool call");
        Err(PortError::InvalidResponse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lettuce_conversations::{ProviderFailureKind, ToolRequest};
    use std::sync::Mutex;

    struct Scripted {
        seen: Mutex<Vec<InferenceRequest>>,
        second: Box<dyn Fn() -> InferenceOutcome + Send + Sync>,
    }

    #[async_trait::async_trait]
    impl InferencePort for Scripted {
        async fn run(&self, request: InferenceRequest) -> Result<InferenceOutcome, PortError> {
            let mut seen = self.seen.lock().expect("seen");
            seen.push(request);
            if seen.len() == 1 {
                // Captured from api.deepseek.com, request id removed.
                return Err(PortError::Provider(ProviderFailure {
                    kind: ProviderFailureKind::RequestRejected,
                    status: 400,
                    code: Some("invalid_request_error".into()),
                    message: Some("Thinking mode does not support this tool_choice".into()),
                    request_id: None,
                }));
            }
            Ok((self.second)())
        }
    }

    fn request() -> InferenceRequest {
        let mut profile = crate::companion::companion_memory_run::tests::profile();
        profile.tool_policy = ToolPolicy::Required;
        InferenceRequest {
            turn_id: lettuce_types::GenerationTurnId::new(),
            attempt_id: lettuce_types::GenerationAttemptId::new(),
            operation: lettuce_conversations::GenerationOperation::Send,
            profile,
            context: lettuce_conversations::ProviderNeutralContext {
                messages: vec![ProviderNeutralMessage {
                    role: MessageRole::User,
                    parts: vec![ProviderContextPart::Text {
                        text: "write".into(),
                    }],
                }],
                attributions: Default::default(),
                budget: Default::default(),
            },
            cancellation: None,
            stream_sink: None,
            media_grants: vec![],
            tools: Some(ToolRequest {
                definitions: vec![lettuce_conversations::ToolDefinition {
                    name: "write_lorebook_entry".into(),
                    description: None,
                    parameters: serde_json::json!({"type": "object"}),
                    version: 1,
                }],
                choice: ToolChoice::Required,
            }),
            prompt_cache_key: None,
        }
    }

    fn outcome(call: Option<&str>) -> InferenceOutcome {
        InferenceOutcome {
            provider_response_id: None,
            candidates: vec![lettuce_conversations::InferenceCandidate {
                ordinal: 0,
                parts: vec![],
                tool_calls: call
                    .map(|name| lettuce_conversations::ProposedToolCall {
                        provider_call_id: None,
                        name: name.into(),
                        arguments: serde_json::json!({"title": "t", "content": "c"}),
                        raw_arguments: None,
                        provider_replay: None,
                    })
                    .into_iter()
                    .collect(),
                provider_replay: None,
                media: vec![],
            }],
            usage: None,
            finish_reason: lettuce_conversations::FinishReason::Stop,
            provider_finish_reason: None,
            provider_request_id: None,
            warning_codes: vec![],
        }
    }

    #[tokio::test]
    async fn thinking_400_retries_once_with_auto_and_an_instruction() {
        let port = Scripted {
            seen: Mutex::new(vec![]),
            second: Box::new(|| outcome(Some("write_lorebook_entry"))),
        };
        let result = run(&port, request()).await.expect("retried");
        assert_eq!(result.candidates[0].tool_calls.len(), 1);
        let seen = port.seen.lock().expect("seen");
        assert_eq!(seen.len(), 2);
        assert_eq!(
            seen[1].tools.as_ref().expect("tools").choice,
            ToolChoice::Auto
        );
        assert_eq!(seen[1].profile.tool_policy, ToolPolicy::Allowed);
        assert!(seen[1].validate().is_ok());
        assert!(format!("{:?}", seen[1].context.messages.last()).contains("write_lorebook_entry"));
    }

    #[tokio::test]
    async fn retry_without_a_tool_call_is_an_invalid_response() {
        let port = Scripted {
            seen: Mutex::new(vec![]),
            second: Box::new(|| outcome(None)),
        };
        assert!(matches!(
            run(&port, request()).await,
            Err(PortError::InvalidResponse)
        ));
    }

    #[test]
    fn only_tool_choice_like_400s_count() {
        let failure = |status, message: &str| ProviderFailure {
            kind: ProviderFailureKind::RequestRejected,
            status,
            code: None,
            message: Some(message.into()),
            request_id: None,
        };
        assert!(is_tool_choice_rejection(&failure(
            400,
            "Thinking mode does not support this tool_choice"
        )));
        assert!(is_tool_choice_rejection(&failure(
            400,
            "tool_choice 'required' is not supported with reasoning"
        )));
        assert!(!is_tool_choice_rejection(&failure(
            400,
            "context length exceeded"
        )));
        assert!(!is_tool_choice_rejection(&failure(401, "thinking")));
    }
}
