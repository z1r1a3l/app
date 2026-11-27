//! Fork (z1r1a3l): live reproduction of the lorebook entry writer against a
//! real DeepSeek account. Ignored by default; run with
//! `LETTUCE_TEST_API_KEY=... cargo test -p lettuce-app fork_entry_live -- --ignored --nocapture`.
//! The key is read from the environment and never printed.

use crate::api::tests::{Reply, harness};
use crate::api::*;
use lettuce_contracts as dto;
use lettuce_conversations::{InferenceOutcome, InferencePort, InferenceRequest};
use lettuce_jobs::{JobStore, WorkerId};
use lettuce_settings::{SecretPurpose, SecretRecord, SecretRef, SecretStore, SecretValue};
use std::sync::{Arc, Mutex};

fn summarize(request: &InferenceRequest) -> String {
    let tools = request.tools.as_ref().map(|tools| {
        (
            tools.choice.clone(),
            tools
                .definitions
                .iter()
                .map(|tool| tool.name.clone())
                .collect::<Vec<_>>(),
        )
    });
    format!(
        "tool_policy={:?} tools={tools:?} messages={}",
        request.profile.tool_policy,
        request.context.messages.len()
    )
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs LETTUCE_TEST_API_KEY (DeepSeek) and network"]
async fn fork_entry_live_deepseek_update_mode() {
    let Ok(key) = std::env::var("LETTUCE_TEST_API_KEY") else {
        return;
    };
    let model = std::env::var("LETTUCE_TEST_MODEL").unwrap_or_else(|_| "deepseek-flash".into());
    let store = Arc::new(lettuce_settings::InMemorySecretStore::new());
    let owner = lettuce_settings::SecretOwnerId::new();
    let key_ref = SecretRef::new();
    store
        .put(
            SecretRecord::new(key_ref, SecretPurpose::ProviderApiKey { owner }),
            SecretValue::new(key).expect("secret"),
            None,
        )
        .await
        .expect("store key");
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let harness = harness(Reply::Text("unused"));
    let own_runtime = Arc::new(lettuce_inference::InferenceRuntime::default());
    let real = Arc::new(lettuce_providers::RemoteProviders::with_runtime(
        store,
        Arc::new(lettuce_network::JsonClient::new().expect("client")),
        own_runtime.clone() as Arc<dyn lettuce_inference::InferenceRuntimePort>,
    ));
    let streaming = std::env::var("LETTUCE_TEST_STREAM").is_ok();
    let max_tokens = std::env::var("LETTUCE_TEST_MAX_TOKENS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());
    {
        let real = real.clone();
        let log = log.clone();
        let model = model.clone();
        let own_runtime = own_runtime.clone();
        *harness.provider.response_hook.lock().expect("hook") = Some(Arc::new(
            move |request: &InferenceRequest, outcome: &mut InferenceOutcome| {
                let mut request = request.clone();
                let profile = &mut request.profile.chat_profile;
                profile.provider_kind = "deepseek".into();
                profile.provider_protocol = lettuce_models::ProviderProtocol::OpenAiCompatible;
                profile.provider_config = lettuce_models::ProviderConfig::Standard;
                profile.endpoint = Some(
                    std::env::var("LETTUCE_TEST_ENDPOINT")
                        .unwrap_or_else(|_| "https://api.deepseek.com".into()),
                );
                profile.external_model_id = model.clone();
                profile.api_key_ref = Some(key_ref);
                profile.secret_owner_id = owner;
                profile.streaming_enabled = streaming;
                request.cancellation = None;
                if let Some(max_tokens) = max_tokens {
                    profile.parameters.visible_max_output_tokens = Some(max_tokens);
                    profile.parameters.total_completion_allowance = None;
                }
                request.stream_sink = None;
                if streaming {
                    let sink = lettuce_types::RequestId::new();
                    let mut receiver = own_runtime.register_stream(sink).expect("sink");
                    tokio::spawn(async move { while receiver.recv().await.is_some() {} });
                    request.stream_sink = Some(sink);
                }
                let summary = summarize(&request);
                let real = real.clone();
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(real.run(request))
                });
                match result {
                    Ok(real_outcome) => {
                        log.lock().expect("log").push(format!(
                            "{summary}\n  -> finish={:?} provider_finish={:?} candidates={:?}",
                            real_outcome.finish_reason,
                            real_outcome.provider_finish_reason,
                            real_outcome.candidates
                        ));
                        *outcome = real_outcome;
                    }
                    Err(error) => {
                        log.lock()
                            .expect("log")
                            .push(format!("{summary}\n  -> ERROR {error:?}"));
                        outcome.candidates.clear();
                        outcome.finish_reason = lettuce_conversations::FinishReason::Error;
                    }
                }
            },
        ));
    }
    let book = lorebook_create(
        &harness.context,
        dto::LorebookCreateRequest {
            client_operation_id: "book".into(),
            metadata: dto::LorebookMetadataInput {
                name: "Мир".into(),
                detection: dto::LorebookDetection::LatestUserMessage,
                icon_asset_id: None,
            },
            entries: vec![
                super::tests::entry("Портовый квартал", "Портовый квартал — шумный район у моря. Над ним возвышается старый маяк. {{user}} часто бывает здесь вместе с {{char}}.", Some("порт")),
                super::tests::entry("Гильдия", "Гильдии города враждуют между собой.", Some("гильдия")),
            ],
        },
    )
    .await
    .expect("book");
    let conversation = crate::api::tests::launch(&harness, "live-source").await;
    let open = conversation_open(
        &harness.context,
        dto::ConversationOpenRequest {
            conversation_id: conversation.clone(),
        },
    )
    .await
    .expect("open");
    let message = conversation_add_user_message(
        &harness.context,
        dto::ConversationAddUserMessageRequest {
            conversation_id: conversation.clone(),
            text: "Я и Ada вернулись. Мы вернулись в Портовый квартал. Старый маяк теперь снесли, а на его месте открыли рынок рыбаков, которым управляет гильдия Серых Сетей.".into(),
            expected_revision: open.revision,
            client_operation_id: "live-message".into(),
        },
    )
    .await
    .expect("message");
    let direction = [
        "UPDATE MODE. Do not create a new entry. Revise the existing lorebook entry below using the selected chat history and memories.",
        "- Return the COMPLETE revised entry (full content, not a diff) and the full keyword list.",
        "",
        "Entry title: Портовый квартал",
        "Entry keywords: порт, квартал",
        "Entry content:",
        "<<<",
        "Портовый квартал — шумный район у моря. Над ним возвышается старый маяк. {{user}} часто бывает здесь вместе с {{char}}.",
        ">>>",
        "",
        "User direction: (none)",
    ]
    .join("\n");
    for (format, force) in [("json", false), ("json", true)] {
        let _ = format;
        let accepted = lorebook_entry_draft(
            &harness.context,
            dto::LorebookEntryDraftRequest {
                client_operation_id: format!("live-{format}-{force}"),
                conversation_id: conversation.clone(),
                lorebook_id: book.lorebook.id.clone(),
                source: dto::LorebookEntryDraftSource::Messages,
                message_ids: vec![message.message.id.clone()],
                memory_ids: vec![],
                use_summary: false,
                direction: Some(direction.clone()),
                force,
            },
        )
        .await
        .expect("admit");
        let db = harness.context.backend().database();
        let job = JobStore::get(db, accepted.job_id.parse().expect("id"))
            .expect("job")
            .expect("exists");
        let work = LorebookHandler
            .claim(&harness.context, &job, WorkerId::new())
            .await
            .expect("claim")
            .expect("work");
        let run = work.run(harness.context.clone(), Arc::new(Progress)).await;
        let view = job_get(
            &harness.context,
            dto::JobGetRequest {
                job_id: accepted.job_id.clone(),
            },
        )
        .await
        .expect("job");
        println!(
            "=== force={force} run={run:?}\nstate={:?} failure={:?} result={:?}",
            view.state, view.failure, view.result
        );
        for line in log.lock().expect("log").drain(..) {
            println!("{line}");
        }
    }
}

struct Progress;
impl JobProgressSink for Progress {
    fn text_delta(&self, _: Option<String>, _: Option<String>) {}
    fn image_progress(&self, _: dto::ImageProgress) {}
}
