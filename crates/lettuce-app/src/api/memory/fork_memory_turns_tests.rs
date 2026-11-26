use crate::api::tests::{Reply, harness, launch};
use crate::api::*;
use lettuce_contracts::{self as dto, ApiErrorCode, ApiErrorDetails};

fn range(start: u64, end: u64) -> dto::MemoryTurnRange {
    dto::MemoryTurnRange { start, end }
}

async fn view(harness: &crate::api::tests::Harness, conversation_id: &str) -> dto::MemoryView {
    memory_get(
        &harness.context,
        dto::ConversationRequest {
            conversation_id: conversation_id.to_owned(),
        },
    )
    .await
    .expect("memory view")
}

#[tokio::test]
async fn manual_add_and_update_round_trip_turn_ranges_on_cycle_label() {
    let harness = harness(Reply::Text("reply"));
    let conversation_id = launch(&harness, "turns-add").await;

    let added = memory_add(
        &harness.context,
        dto::MemoryAddRequest {
            conversation_id: conversation_id.clone(),
            text: "They met at the market.".into(),
            category: None,
            observed_at: None,
            expected_revision: 1,
            client_operation_id: "turns-add-range".into(),
            perspective_character_id: None,
            turns: Some(range(10, 20)),
        },
    )
    .await
    .expect("add with turns");
    let memory_id = added.memory_id.expect("id");
    let listed = view(&harness, &conversation_id).await;
    let item = listed
        .items
        .iter()
        .find(|item| item.id == memory_id)
        .expect("item");
    assert_eq!(item.cycle_label.as_deref(), Some("10-20"));

    memory_update(
        &harness.context,
        dto::MemoryUpdateRequest {
            conversation_id: conversation_id.clone(),
            memory_id: memory_id.clone(),
            text: None,
            category: dto::MemoryCategoryChange::Keep,
            observed_at: dto::MemoryObservedAtChange::Keep,
            turns: dto::MemoryTurnsChange::Set(Some(range(7, 7))),
            expected_revision: listed.revision,
            client_operation_id: "turns-update-single".into(),
            perspective_character_id: None,
        },
    )
    .await
    .expect("update to a single turn");
    assert_eq!(
        view(&harness, &conversation_id)
            .await
            .items
            .iter()
            .find(|item| item.id == memory_id)
            .expect("item")
            .cycle_label
            .as_deref(),
        Some("7-7")
    );

    let before_clear = view(&harness, &conversation_id).await;
    memory_update(
        &harness.context,
        dto::MemoryUpdateRequest {
            conversation_id: conversation_id.clone(),
            memory_id: memory_id.clone(),
            text: Some("They met at the market, at dusk.".into()),
            category: dto::MemoryCategoryChange::Keep,
            observed_at: dto::MemoryObservedAtChange::Keep,
            turns: dto::MemoryTurnsChange::Keep,
            expected_revision: before_clear.revision,
            client_operation_id: "turns-keep-text".into(),
            perspective_character_id: None,
        },
    )
    .await
    .expect("keep turns while editing text");
    assert_eq!(
        view(&harness, &conversation_id)
            .await
            .items
            .iter()
            .find(|item| item.id == memory_id)
            .expect("item")
            .cycle_label
            .as_deref(),
        Some("7-7")
    );

    let before_wipe = view(&harness, &conversation_id).await;
    memory_update(
        &harness.context,
        dto::MemoryUpdateRequest {
            conversation_id: conversation_id.clone(),
            memory_id: memory_id.clone(),
            text: None,
            category: dto::MemoryCategoryChange::Keep,
            observed_at: dto::MemoryObservedAtChange::Keep,
            turns: dto::MemoryTurnsChange::Set(None),
            expected_revision: before_wipe.revision,
            client_operation_id: "turns-clear".into(),
            perspective_character_id: None,
        },
    )
    .await
    .expect("clear turns");
    assert_eq!(
        view(&harness, &conversation_id)
            .await
            .items
            .iter()
            .find(|item| item.id == memory_id)
            .expect("item")
            .cycle_label,
        None
    );
}

#[tokio::test]
async fn manual_add_without_turns_has_no_cycle_label() {
    let harness = harness(Reply::Text("reply"));
    let conversation_id = launch(&harness, "turns-empty").await;
    memory_add(
        &harness.context,
        dto::MemoryAddRequest {
            conversation_id: conversation_id.clone(),
            text: "A note with no turns.".into(),
            category: None,
            observed_at: None,
            expected_revision: 1,
            client_operation_id: "turns-empty-add".into(),
            perspective_character_id: None,
            turns: None,
        },
    )
    .await
    .expect("add");
    assert_eq!(
        view(&harness, &conversation_id).await.items[0].cycle_label,
        None
    );
}

#[tokio::test]
async fn invalid_turn_ranges_are_rejected_and_write_nothing() {
    let harness = harness(Reply::Text("reply"));
    let conversation_id = launch(&harness, "turns-invalid").await;

    for (key, turns) in [("zero", range(0, 10)), ("reversed", range(20, 10))] {
        let error = memory_add(
            &harness.context,
            dto::MemoryAddRequest {
                conversation_id: conversation_id.clone(),
                text: "Should not be stored.".into(),
                category: None,
                observed_at: None,
                expected_revision: 1,
                client_operation_id: format!("turns-invalid-{key}"),
                perspective_character_id: None,
                turns: Some(turns),
            },
        )
        .await
        .expect_err(key);
        assert_eq!(error.code, ApiErrorCode::InvalidInput);
        assert_eq!(
            error.details,
            Some(ApiErrorDetails::InvalidField {
                field: "turns".into()
            })
        );
    }
    assert!(view(&harness, &conversation_id).await.items.is_empty());

    let added = memory_add(
        &harness.context,
        dto::MemoryAddRequest {
            conversation_id: conversation_id.clone(),
            text: "Valid first.".into(),
            category: None,
            observed_at: None,
            expected_revision: 1,
            client_operation_id: "turns-invalid-seed".into(),
            perspective_character_id: None,
            turns: Some(range(1, 3)),
        },
    )
    .await
    .expect("seed");
    let listed = view(&harness, &conversation_id).await;
    let error = memory_update(
        &harness.context,
        dto::MemoryUpdateRequest {
            conversation_id: conversation_id.clone(),
            memory_id: added.memory_id.expect("id"),
            text: Some("Should not change.".into()),
            category: dto::MemoryCategoryChange::Keep,
            observed_at: dto::MemoryObservedAtChange::Keep,
            turns: dto::MemoryTurnsChange::Set(Some(range(8, 2))),
            expected_revision: listed.revision,
            client_operation_id: "turns-invalid-update".into(),
            perspective_character_id: None,
        },
    )
    .await
    .expect_err("reversed update");
    assert_eq!(error.code, ApiErrorCode::InvalidInput);
    let after = view(&harness, &conversation_id).await;
    assert_eq!(after.items[0].text, "Valid first.");
    assert_eq!(after.items[0].cycle_label.as_deref(), Some("1-3"));
    assert_eq!(after.revision, listed.revision);
}

#[tokio::test]
async fn deleting_a_memory_clears_its_stored_turns() {
    let harness = harness(Reply::Text("reply"));
    let conversation_id = launch(&harness, "turns-delete").await;
    let added = memory_add(
        &harness.context,
        dto::MemoryAddRequest {
            conversation_id: conversation_id.clone(),
            text: "To be removed.".into(),
            category: None,
            observed_at: None,
            expected_revision: 1,
            client_operation_id: "turns-delete-add".into(),
            perspective_character_id: None,
            turns: Some(range(4, 6)),
        },
    )
    .await
    .expect("add");
    let memory_id = added.memory_id.expect("id");
    let listed = view(&harness, &conversation_id).await;
    memory_delete(
        &harness.context,
        dto::MemoryDeleteRequest {
            conversation_id: conversation_id.clone(),
            memory_id: memory_id.clone(),
            expected_revision: listed.revision,
            client_operation_id: "turns-delete-now".into(),
            perspective_character_id: None,
        },
    )
    .await
    .expect("delete");
    assert!(
        harness
            .context
            .backend()
            .database()
            .fork_memory_turns_get(&memory_id)
            .expect("stored")
            .is_none()
    );
}

#[tokio::test]
async fn add_with_turns_replays_the_same_receipt() {
    let harness = harness(Reply::Text("reply"));
    let conversation_id = launch(&harness, "turns-replay").await;
    let request = dto::MemoryAddRequest {
        conversation_id: conversation_id.clone(),
        text: "Replayed with turns.".into(),
        category: None,
        observed_at: None,
        expected_revision: 1,
        client_operation_id: "turns-replay-add".into(),
        perspective_character_id: None,
        turns: Some(range(11, 12)),
    };
    let first = memory_add(&harness.context, request.clone())
        .await
        .expect("add");
    assert_eq!(
        memory_add(&harness.context, request).await.expect("replay"),
        first
    );
    assert_eq!(
        view(&harness, &conversation_id).await.items[0]
            .cycle_label
            .as_deref(),
        Some("11-12")
    );
}
