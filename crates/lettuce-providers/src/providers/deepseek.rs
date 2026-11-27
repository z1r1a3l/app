use crate::descriptor::{
    ApiKeyRequirement, ParameterFlags, PromptCachingSupport, ProviderDescriptor, ReasoningSupport,
};
use crate::providers::openai_compatible::OpenAiWireProvider;

pub(crate) struct DeepSeek;

impl OpenAiWireProvider for DeepSeek {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        &DESCRIPTOR
    }

    fn reasoning_policy(&self) -> crate::providers::openai_compatible::ReasoningWirePolicy {
        crate::providers::openai_compatible::ReasoningWirePolicy::MaxTokens
    }

    /// DeepSeek V4 models think by default, and thinking mode rejects a forced
    /// `tool_choice` ("required" or a named function) with HTTP 400. Requests
    /// that force a tool (background jobs such as the lorebook writers) turn
    /// thinking off so the forced call is accepted; chat requests are untouched.
    fn extend_body(
        &self,
        _parameters: &lettuce_models::ResolvedChatParameters,
        body: &mut serde_json::Map<String, serde_json::Value>,
    ) {
        let forced = body
            .get("tool_choice")
            .is_some_and(|choice| choice.as_str() != Some("auto") && !choice.is_null());
        if forced && !body.contains_key("thinking") {
            body.insert(
                "thinking".to_owned(),
                serde_json::json!({ "type": "disabled" }),
            );
        }
    }
}

pub(crate) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    kind: "deepseek",
    display_name: "DeepSeek",
    protocol: lettuce_models::ProviderProtocol::OpenAiCompatible,
    aliases: &[],
    default_endpoint: Some("https://api.deepseek.com"),
    endpoint_editable: false,
    api_key: ApiKeyRequirement::Required,
    auth_header: "Authorization",
    streaming: true,
    lists_models: true,
    verifies_key: true,
    reasoning: ReasoningSupport::None,
    prompt_caching: PromptCachingSupport::None,
    parameters: ParameterFlags::PENALTIES,
    extra_body_keys: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn body(tool_choice: Option<serde_json::Value>) -> serde_json::Map<String, serde_json::Value> {
        let mut body = serde_json::Map::new();
        if let Some(choice) = tool_choice {
            body.insert("tool_choice".to_owned(), choice);
        }
        DeepSeek.extend_body(&crate::integration_tests::parameters(), &mut body);
        body
    }

    #[test]
    fn forced_tool_choice_turns_thinking_off_and_chat_is_untouched() {
        // HTTP 400 from api.deepseek.com otherwise:
        // {"error":{"message":"Thinking mode does not support this tool_choice",
        //  "type":"invalid_request_error","code":"invalid_request_error"}}
        for choice in [
            serde_json::json!("required"),
            serde_json::json!({"type": "function", "function": {"name": "write_lorebook_entry"}}),
        ] {
            assert_eq!(
                body(Some(choice))["thinking"],
                serde_json::json!({"type": "disabled"})
            );
        }
        assert!(
            body(Some(serde_json::json!("auto")))
                .get("thinking")
                .is_none()
        );
        assert!(body(None).get("thinking").is_none());
    }
}
