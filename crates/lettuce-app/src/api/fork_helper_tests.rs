//! Fork-only: helper model end to end: a read after a reply, its state in
//! the next prompt, rollback on regenerate, manual edits and settings.

use std::sync::Arc;

use lettuce_contracts as dto;
use lettuce_conversations::{InferenceRequest, MessagePart, ProviderContextPart};

use super::*;
use crate::api::conversations::turns_tests::{regenerate_request, replied_chat, run_generation};
use crate::api::jobs::{JobHandlers, JobRunner};
use crate::api::tests::{Harness, RecordingStream, Reply, harness, send};
use crate::api::*;

const HELPER_ANSWER: &str = "<state_update>\n[Scene]\nМесто: таверна\n[Characters]\nAqua · Одежда: синее платье\n[Inventory]\n+ Traveller: посох\n[Notebook]\n+ T1: Аква — богиня | hidden from: Traveller\nClue T1: smiles warmly\n</state_update>";

fn text_of(request: &InferenceRequest) -> String {
    request
        .context
        .messages
        .iter()
        .flat_map(|message| &message.parts)
        .filter_map(|part| match part {
            ProviderContextPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_helper(request: &InferenceRequest) -> bool {
    text_of(request).contains("Write the change lines now")
}

/// The model answers the helper with `HELPER_ANSWER` and the chat with the
/// harness's reply.
fn helper_harness() -> Harness {
    helper_harness_with(HELPER_ANSWER)
}

fn helper_harness_with(answer: &'static str) -> Harness {
    let harness = harness(Reply::Text("Aqua smiles warmly at you."));
    *harness.provider.response_hook.lock().expect("hook") =
        Some(Arc::new(move |request, outcome| {
            if is_helper(request) {
                outcome.candidates[0].parts = vec![MessagePart::Text {
                    text: answer.into(),
                }];
            }
        }));
    harness
}

async fn run_jobs(harness: &Harness) {
    let runner = JobRunner::new(harness.context.clone(), JobHandlers::standard());
    // Reads of one chat share a lane: wait for each before the next.
    while runner.run_once().await.expect("runner") {
        runner.wait_idle().await;
    }
    runner.wait_idle().await;
}

fn all() -> dto::HelperTrackersDto {
    dto::HelperTrackersDto {
        scene: true,
        characters: true,
        bonds: true,
        inventory: true,
        npcs: true,
        notebook: true,
    }
}

async fn switch_on(harness: &Harness, chat: &str) {
    helper_scope_set(
        &harness.context,
        dto::HelperScopeUpdate {
            scope: "conversation".into(),
            id: chat.into(),
            enabled: true,
            trackers: all(),
        },
    )
    .await
    .expect("switch on");
}

async fn admit(harness: &Harness, chat: &str) -> Option<lettuce_types::JobId> {
    let id: lettuce_types::ConversationId = chat.parse().expect("chat");
    harness
        .context
        .blocking(move |context| crate::api::jobs::fork_helper::admit_latest(context, id))
        .await
        .expect("admit")
}

async fn chat_view(harness: &Harness, chat: &str) -> dto::HelperChatView {
    helper_chat_get(
        &harness.context,
        dto::HelperChatRequest {
            conversation_id: chat.into(),
        },
    )
    .await
    .expect("chat view")
}

fn value<'a>(entities: &'a [dto::HelperEntityDto], name: &str, field: &str) -> Option<&'a str> {
    entities
        .iter()
        .find(|entity| entity.name == name)?
        .fields
        .iter()
        .find(|f| f.name == field)
        .map(|f| f.value.as_str())
}

fn helper_requests(harness: &Harness) -> usize {
    harness
        .provider
        .requests
        .lock()
        .expect("requests")
        .iter()
        .filter(|request| is_helper(request))
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_helper_is_off_until_switched_on() {
    let harness = helper_harness();
    let (chat, _) = replied_chat(&harness, "helper-off").await;
    assert_eq!(admit(&harness, &chat).await, None);
    let view = chat_view(&harness, &chat).await;
    assert!(!view.enabled && view.chat.is_none() && view.status.is_none());
    run_jobs(&harness).await;
    assert_eq!(helper_requests(&harness), 0, "no helper call while off");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_stores_changes_and_the_next_prompt_gets_one_state_block() {
    let harness = helper_harness();
    let (chat, reply) = replied_chat(&harness, "helper-read").await;
    switch_on(&harness, &chat).await;
    let job = admit(&harness, &chat).await.expect("queued");
    run_jobs(&harness).await;

    let view = chat_view(&harness, &chat).await;
    assert!(view.enabled);
    let status = view.status.expect("status");
    assert_eq!(status.job_id, job.to_string());
    assert_eq!(status.state, dto::HelperRunState::Done, "{status:?}");
    assert_eq!(
        value(&view.state.characters, "Aqua", "Одежда"),
        Some("синее платье")
    );
    assert_eq!(view.state.scene[0].value, "таверна");
    assert_eq!(view.state.inventory[0].items, ["посох"]);
    let truth = &view.state.notebook[0];
    assert_eq!((truth.id.as_str(), truth.revealed), ("T1", false));
    assert_eq!(truth.clues[0].found, Some(true), "the clue is in the reply");

    let read = helper_message(
        &harness.context,
        dto::HelperMessageRequest {
            conversation_id: chat.clone(),
            message_id: reply.clone(),
        },
    )
    .await
    .expect("message");
    assert!(read.read);
    assert!(read.raw.contains("синее платье"));
    assert!(
        read.changes
            .iter()
            .any(|c| c.kind == "set" && c.subject == "Aqua" && c.value == "синее платье")
    );

    // The helper read the reply and the user message before it.
    let requests = harness.provider.requests.lock().expect("requests").clone();
    let helper_text = text_of(requests.iter().find(|r| is_helper(r)).expect("helper call"));
    assert!(helper_text.contains("Aqua smiles warmly at you."));
    assert!(helper_text.contains("Hi there"));
    assert!(helper_text.contains("NSFW/NSFL allowed; all descriptions explicit."));

    // The next turn: the state block is in the prompt once; the post-reply
    // hook queues the next read by itself.
    send(
        &harness,
        &chat,
        "helper-read-2",
        "Hi again",
        Arc::new(RecordingStream::default()),
    )
    .await
    .expect("send");
    run_generation(&harness).await;
    let requests = harness.provider.requests.lock().expect("requests").clone();
    let main = requests
        .iter()
        .rev()
        .find(|r| !is_helper(r) && text_of(r).contains("Hi again"))
        .expect("main request");
    let prompt = text_of(main);
    assert_eq!(prompt.matches("[Current state").count(), 1, "{prompt}");
    assert!(prompt.contains("синее платье"), "{prompt}");
    assert!(prompt.contains("Аква — богиня"), "{prompt}");
    assert!(!prompt.contains("<state_update>"), "{prompt}");
    let status = chat_view(&harness, &chat).await.status.expect("status");
    assert_eq!(status.state, dto::HelperRunState::Queued);
    assert_ne!(status.job_id, job.to_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn regenerating_a_reply_rolls_its_changes_back_and_drops_a_stale_read() {
    let harness = helper_harness();
    let (chat, reply) = replied_chat(&harness, "helper-regen").await;
    switch_on(&harness, &chat).await;
    admit(&harness, &chat).await.expect("queued");
    run_jobs(&harness).await;
    assert!(!chat_view(&harness, &chat).await.state.characters.is_empty());

    // A read queued for the old variant, then the reply is regenerated.
    admit(&harness, &chat).await.expect("queued again");
    conversation_regenerate(
        &harness.context,
        regenerate_request(&harness, &chat, &reply, "helper-regen-1"),
        Arc::new(RecordingStream::default()),
    )
    .await
    .expect("regenerate");
    run_generation(&harness).await;
    let requests = harness.provider.requests.lock().expect("requests").clone();
    let regen = text_of(
        requests
            .iter()
            .rev()
            .find(|r| !is_helper(r))
            .expect("regenerate request"),
    );
    assert!(
        !regen.contains("синее платье"),
        "the replaced reply's changes are not in its regenerate prompt: {regen}"
    );

    let view = chat_view(&harness, &chat).await;
    assert!(
        view.state.characters.is_empty(),
        "rolled back: {:?}",
        view.state
    );
    assert!(view.state.notebook.is_empty());

    // The stale read is dropped; the hook's read of the new variant stores.
    let before = helper_requests(&harness);
    run_jobs(&harness).await;
    assert_eq!(
        helper_requests(&harness),
        before + 1,
        "the stale read never called the model"
    );
    let view = chat_view(&harness, &chat).await;
    assert_eq!(
        value(&view.state.characters, "Aqua", "Одежда"),
        Some("синее платье")
    );
    let rows = harness
        .context
        .backend()
        .database()
        .fork_helper_deltas(&chat)
        .expect("rows");
    assert_eq!(rows.len(), 2, "one read per variant");
}

#[tokio::test(flavor = "multi_thread")]
async fn manual_edits_are_kept_with_the_latest_reply() {
    let harness = helper_harness();
    let (chat, reply) = replied_chat(&harness, "helper-edit").await;
    switch_on(&harness, &chat).await;
    admit(&harness, &chat).await.expect("queued");
    run_jobs(&harness).await;
    let mut state = chat_view(&harness, &chat).await.state;
    state.characters[0].fields[0].value = "красная мантия".into();
    state.npcs.push(dto::HelperEntityDto {
        name: "Трактирщик".into(),
        fields: vec![dto::HelperFieldDto {
            name: "Настроение".into(),
            value: "хмурый".into(),
        }],
        locked: false,
    });
    state.notebook[0].revealed = true;
    let view = helper_state_set(
        &harness.context,
        dto::HelperStateUpdate {
            conversation_id: chat.clone(),
            state,
        },
    )
    .await
    .expect("save");
    assert_eq!(
        value(&view.state.characters, "Aqua", "Одежда"),
        Some("красная мантия")
    );
    assert_eq!(
        value(&view.state.npcs, "Трактирщик", "Настроение"),
        Some("хмурый")
    );
    assert!(view.state.notebook[0].revealed);
    let read = helper_message(
        &harness.context,
        dto::HelperMessageRequest {
            conversation_id: chat.clone(),
            message_id: reply.clone(),
        },
    )
    .await
    .expect("message");
    assert_eq!(read.edits, 1);

    // Saving the same state again adds nothing.
    let same = view.state.clone();
    helper_state_set(
        &harness.context,
        dto::HelperStateUpdate {
            conversation_id: chat.clone(),
            state: same,
        },
    )
    .await
    .expect("save again");
    assert_eq!(
        harness
            .context
            .backend()
            .database()
            .fork_helper_deltas(&chat)
            .expect("rows")
            .len(),
        2
    );

    // Regenerating the reply undoes the edit along with the reply's read.
    conversation_regenerate(
        &harness.context,
        regenerate_request(&harness, &chat, &reply, "helper-edit-regen"),
        Arc::new(RecordingStream::default()),
    )
    .await
    .expect("regenerate");
    run_generation(&harness).await;
    let view = chat_view(&harness, &chat).await;
    assert!(view.state.npcs.is_empty() && view.state.characters.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tracker_switched_off_keeps_its_lines_out() {
    let harness = helper_harness();
    let (chat, _) = replied_chat(&harness, "helper-trackers").await;
    helper_scope_set(
        &harness.context,
        dto::HelperScopeUpdate {
            scope: "conversation".into(),
            id: chat.clone(),
            enabled: true,
            trackers: dto::HelperTrackersDto {
                notebook: false,
                inventory: false,
                ..all()
            },
        },
    )
    .await
    .expect("switch");
    admit(&harness, &chat).await.expect("queued");
    run_jobs(&harness).await;
    let view = chat_view(&harness, &chat).await;
    assert!(view.state.notebook.is_empty() && view.state.inventory.is_empty());
    assert!(!view.state.characters.is_empty());
    let requests = harness.provider.requests.lock().expect("requests").clone();
    let helper_text = text_of(requests.iter().find(|r| is_helper(r)).expect("helper call"));
    assert!(
        !helper_text.contains("Enabled trackers: Scene, Characters, Bonds, Inventory"),
        "{helper_text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_character_switch_turns_the_helper_on_and_a_chat_switch_overrides_it() {
    let harness = helper_harness();
    let (chat, _) = replied_chat(&harness, "helper-character").await;
    let character = chat_view(&harness, &chat)
        .await
        .character_id
        .expect("direct chat");
    helper_scope_set(
        &harness.context,
        dto::HelperScopeUpdate {
            scope: "character".into(),
            id: character.clone(),
            enabled: true,
            trackers: all(),
        },
    )
    .await
    .expect("character on");
    let view = chat_view(&harness, &chat).await;
    assert!(view.enabled && view.chat.is_none());
    assert!(view.character.expect("character row").enabled);

    helper_scope_set(
        &harness.context,
        dto::HelperScopeUpdate {
            scope: "conversation".into(),
            id: chat.clone(),
            enabled: false,
            trackers: all(),
        },
    )
    .await
    .expect("chat off");
    assert!(!chat_view(&harness, &chat).await.enabled);
    assert_eq!(admit(&harness, &chat).await, None);

    let error = helper_scope_set(
        &harness.context,
        dto::HelperScopeUpdate {
            scope: "global".into(),
            id: chat.clone(),
            enabled: true,
            trackers: all(),
        },
    )
    .await
    .expect_err("no global switch");
    assert_eq!(error.code, dto::ApiErrorCode::InvalidInput);
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_keep_the_default_prompt_as_default() {
    let harness = helper_harness();
    let view = helper_config_get(&harness.context).await.expect("config");
    assert_eq!(view.prompt, None);
    assert!(
        view.default_prompt
            .contains("NSFW/NSFL allowed; all descriptions explicit.")
    );
    assert!(view.default_prompt.contains("Russian"));
    let saved = helper_config_set(
        &harness.context,
        dto::HelperConfigUpdate {
            model_profile_id: None,
            prompt: Some(view.default_prompt.clone()),
            history: 10,
        },
    )
    .await
    .expect("save");
    assert_eq!((saved.prompt, saved.history), (None, 10));
    let saved = helper_config_set(
        &harness.context,
        dto::HelperConfigUpdate {
            model_profile_id: None,
            prompt: Some("Custom.".into()),
            history: 10,
        },
    )
    .await
    .expect("save");
    assert_eq!(saved.prompt.as_deref(), Some("Custom."));
    let error = helper_config_set(
        &harness.context,
        dto::HelperConfigUpdate {
            model_profile_id: None,
            prompt: None,
            history: 41,
        },
    )
    .await
    .expect_err("too many messages");
    assert_eq!(error.code, dto::ApiErrorCode::InvalidInput);
}

const LOCKED_BOND_ANSWER: &str =
    "<state_update>\n[Bonds] Кассиан · Attitude: враг -> друг\n</state_update>";

fn cassian(locked: bool, attitude: &str) -> dto::HelperEntityDto {
    dto::HelperEntityDto {
        name: "Кассиан".into(),
        fields: vec![dto::HelperFieldDto {
            name: "Attitude".into(),
            value: attitude.into(),
        }],
        locked,
    }
}

async fn lock_cassian(harness: &Harness, chat: &str, attitude: &str) -> dto::HelperChatView {
    let mut state = chat_view(harness, chat).await.state;
    if let Some(bond) = state.bonds.iter_mut().find(|bond| bond.name == "Кассиан") {
        bond.fields = cassian(true, attitude).fields;
        bond.locked = true;
    } else {
        state.bonds.push(cassian(true, attitude));
    }
    helper_state_set(
        &harness.context,
        dto::HelperStateUpdate {
            conversation_id: chat.into(),
            state,
        },
    )
    .await
    .expect("lock")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_locked_bond_ignores_helper_changes_and_survives_regenerate() {
    let harness = helper_harness_with(LOCKED_BOND_ANSWER);
    let (chat, reply) = replied_chat(&harness, "helper-lock").await;
    switch_on(&harness, &chat).await;
    let view = lock_cassian(&harness, &chat, "враг").await;
    assert_eq!(
        value(&view.state.bonds, "Кассиан", "Attitude"),
        Some("враг")
    );
    assert!(
        view.state
            .bonds
            .iter()
            .any(|bond| bond.name == "Кассиан" && bond.locked),
        "{:?}",
        view.state.bonds
    );

    admit(&harness, &chat).await.expect("queued");
    run_jobs(&harness).await;
    let view = chat_view(&harness, &chat).await;
    assert_eq!(
        value(&view.state.bonds, "Кассиан", "Attitude"),
        Some("враг")
    );
    assert!(
        view.state
            .bonds
            .iter()
            .any(|bond| bond.name == "Кассиан" && bond.locked)
    );

    let requests = harness.provider.requests.lock().expect("requests").clone();
    let helper_text = text_of(
        requests
            .iter()
            .rev()
            .find(|r| is_helper(r))
            .expect("helper call"),
    );
    assert!(
        helper_text.contains("Locked bonds (do not change or remove): Кассиан"),
        "{helper_text}"
    );
    assert!(helper_text.contains("[locked]"), "{helper_text}");

    conversation_regenerate(
        &harness.context,
        regenerate_request(&harness, &chat, &reply, "helper-lock-regen"),
        Arc::new(RecordingStream::default()),
    )
    .await
    .expect("regenerate");
    run_generation(&harness).await;
    let view = chat_view(&harness, &chat).await;
    assert_eq!(
        value(&view.state.bonds, "Кассиан", "Attitude"),
        Some("враг"),
        "lock snapshot survives swipe: {:?}",
        view.state.bonds
    );
    assert!(
        view.state
            .bonds
            .iter()
            .any(|bond| bond.name == "Кассиан" && bond.locked)
    );

    run_jobs(&harness).await;
    let view = chat_view(&harness, &chat).await;
    assert_eq!(
        value(&view.state.bonds, "Кассиан", "Attitude"),
        Some("враг")
    );
    assert!(
        view.state
            .bonds
            .iter()
            .any(|bond| bond.name == "Кассиан" && bond.locked)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_user_can_edit_or_unlock_a_locked_bond() {
    let harness = helper_harness_with(LOCKED_BOND_ANSWER);
    let (chat, _) = replied_chat(&harness, "helper-lock-edit").await;
    switch_on(&harness, &chat).await;
    lock_cassian(&harness, &chat, "враг").await;

    let mut state = chat_view(&harness, &chat).await.state;
    state.bonds = vec![cassian(true, "нейтрален")];
    let view = helper_state_set(
        &harness.context,
        dto::HelperStateUpdate {
            conversation_id: chat.clone(),
            state,
        },
    )
    .await
    .expect("edit locked");
    assert_eq!(
        value(&view.state.bonds, "Кассиан", "Attitude"),
        Some("нейтрален")
    );
    assert!(
        view.state
            .bonds
            .iter()
            .any(|bond| bond.name == "Кассиан" && bond.locked)
    );

    let mut state = view.state;
    state.bonds = vec![cassian(false, "нейтрален")];
    let view = helper_state_set(
        &harness.context,
        dto::HelperStateUpdate {
            conversation_id: chat.clone(),
            state,
        },
    )
    .await
    .expect("unlock");
    assert_eq!(
        value(&view.state.bonds, "Кассиан", "Attitude"),
        Some("нейтрален")
    );
    assert!(
        view.state
            .bonds
            .iter()
            .any(|bond| bond.name == "Кассиан" && !bond.locked)
    );

    admit(&harness, &chat).await.expect("queued");
    run_jobs(&harness).await;
    let view = chat_view(&harness, &chat).await;
    assert_eq!(
        value(&view.state.bonds, "Кассиан", "Attitude"),
        Some("друг"),
        "helper may change an unlocked bond: {:?}",
        view.state.bonds
    );
    assert!(
        view.state
            .bonds
            .iter()
            .any(|bond| bond.name == "Кассиан" && !bond.locked)
    );
}
