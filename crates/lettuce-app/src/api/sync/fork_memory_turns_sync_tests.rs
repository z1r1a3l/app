// fork: user-set memory turn ranges reach the other device, and
// edits and clears made there come back.

use super::*;
use crate::api::tests::launch;

fn range(start: u64, end: u64) -> dto::MemoryTurnRange {
    dto::MemoryTurnRange { start, end }
}

async fn view(context: &ApiContext, conversation_id: &str) -> dto::MemoryView {
    memory_get(
        context,
        dto::ConversationRequest {
            conversation_id: conversation_id.to_owned(),
        },
    )
    .await
    .expect("memory view")
}

fn label_of(view: &dto::MemoryView, memory_id: &str) -> Option<String> {
    view.items
        .iter()
        .find(|item| item.id == memory_id)
        .and_then(|item| item.cycle_label.clone())
}

#[tokio::test(flavor = "multi_thread")]
async fn fork_memory_turns_sync_both_ways() {
    let (host, host_root) = device();
    let (client, client_root) = device();
    let conversation_id = launch(&host, "turns-sync-launch").await;
    let added = memory_add(
        &host.context,
        dto::MemoryAddRequest {
            conversation_id: conversation_id.clone(),
            text: "They met at the market.".into(),
            category: None,
            observed_at: None,
            expected_revision: 1,
            client_operation_id: "turns-sync-add".into(),
            perspective_character_id: None,
            turns: Some(range(10, 20)),
        },
    )
    .await
    .expect("host add");
    let memory_id = added.memory_id.expect("id");
    assert_eq!(
        label_of(&view(&host.context, &conversation_id).await, &memory_id).as_deref(),
        Some("10-20"),
        "host should store the range before sync"
    );
    complete(&host.context, &client.context).await;
    let listed = view(&client.context, &conversation_id).await;
    assert!(
        listed.items.iter().any(|item| item.id == memory_id),
        "client missing memory {memory_id}; items={:?}",
        listed
            .items
            .iter()
            .map(|item| (item.id.clone(), item.text.clone(), item.cycle_label.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        client
            .context
            .backend()
            .database()
            .fork_memory_turns_get(&memory_id)
            .expect("client row")
            .map(|range| range.label())
            .as_deref(),
        Some("10-20"),
        "turns row missing on the client; cycle_label={:?}",
        label_of(&listed, &memory_id)
    );
    assert_eq!(label_of(&listed, &memory_id).as_deref(), Some("10-20"));

    let listed = view(&client.context, &conversation_id).await;
    memory_update(
        &client.context,
        dto::MemoryUpdateRequest {
            conversation_id: conversation_id.clone(),
            memory_id: memory_id.clone(),
            text: None,
            category: dto::MemoryCategoryChange::Keep,
            observed_at: dto::MemoryObservedAtChange::Keep,
            turns: dto::MemoryTurnsChange::Set(Some(range(7, 7))),
            expected_revision: listed.revision,
            client_operation_id: "turns-sync-edit".into(),
            perspective_character_id: None,
        },
    )
    .await
    .expect("client edit");
    complete(&host.context, &client.context).await;
    assert_eq!(
        label_of(&view(&host.context, &conversation_id).await, &memory_id).as_deref(),
        Some("7-7")
    );

    let listed = view(&client.context, &conversation_id).await;
    memory_update(
        &client.context,
        dto::MemoryUpdateRequest {
            conversation_id: conversation_id.clone(),
            memory_id: memory_id.clone(),
            text: None,
            category: dto::MemoryCategoryChange::Keep,
            observed_at: dto::MemoryObservedAtChange::Keep,
            turns: dto::MemoryTurnsChange::Set(None),
            expected_revision: listed.revision,
            client_operation_id: "turns-sync-clear".into(),
            perspective_character_id: None,
        },
    )
    .await
    .expect("client clear");
    complete(&host.context, &client.context).await;
    assert_eq!(
        label_of(&view(&host.context, &conversation_id).await, &memory_id),
        None
    );

    sync_stop(&host.context).await.expect("stop host");
    sync_stop(&client.context).await.expect("stop client");
    std::fs::remove_dir_all(host_root).expect("host cleanup");
    std::fs::remove_dir_all(client_root).expect("client cleanup");
}
