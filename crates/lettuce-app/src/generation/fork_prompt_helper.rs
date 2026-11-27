//! Fork-only: the helper model's tracked state in the main prompt.
//!
//! When the helper is on for a chat, one compact "Current state" block (the
//! fold of the shown path's tracker changes) goes in as a system message
//! just before the latest message. The reply being regenerated or continued
//! does not count: its own changes are rolled back for the new attempt.
//! Old tracker blocks never enter the prompt, because the helper's answers
//! are stored apart from the messages.

use lettuce_context::fork_helper::{self as helper, HelperState, StoredDelta, apply_bond_locks};
use lettuce_conversations::{
    ContextRequest, MessageRenderSource, MessageRole, ProviderContextPart, ProviderNeutralMessage,
    TimelineItem,
};

/// Depth of the block: before the last message of the history.
const STATE_DEPTH: u32 = 1;

pub(crate) const STATE_HEADER: &str = "[Current state: kept by the app from earlier replies. Treat as true. Do not write tracker or state blocks; story prose only.]";

/// The variant a message shows: `c:<candidate>` or `r:<revision>`.
pub(crate) fn variant_key(item: &TimelineItem) -> String {
    match item.message.active_render_source {
        MessageRenderSource::Candidate(id) => format!("c:{id}"),
        MessageRenderSource::Revision(id) => format!("r:{id}"),
    }
}

/// `(message id, shown variant)` of each item, in order.
pub(crate) fn path_of<'a>(
    items: impl IntoIterator<Item = &'a TimelineItem>,
) -> Vec<(String, String)> {
    items
        .into_iter()
        .map(|item| (item.message.id.to_string(), variant_key(item)))
        .collect()
}

/// The state the main model should see for `request`.
pub(crate) fn state_for_request(request: &ContextRequest, rows: &[StoredDelta]) -> HelperState {
    let skip = request
        .timeline
        .iter()
        .find(|item| {
            item.message.id == request.source_message_id
                && item.message.role == MessageRole::Assistant
        })
        .map(|item| item.message.id);
    let path = path_of(
        request
            .timeline
            .iter()
            .filter(|item| Some(item.message.id) != skip),
    );
    helper::state_for_path(rows, &path)
}

/// Adds the block to `in_chat` when the helper is on and has state.
pub(crate) fn inject(
    in_chat: &mut Vec<(u32, ProviderNeutralMessage)>,
    request: &ContextRequest,
    character_ids: &[String],
) {
    let Some(source) = helper::state_source() else {
        return;
    };
    let conversation_id = request.conversation_id.to_string();
    let Some(trackers) = source.trackers(&conversation_id, character_ids) else {
        return;
    };
    let rows = source.deltas(&conversation_id);
    let locks = source.bond_locks(&conversation_id);
    if rows.is_empty() && locks.is_empty() {
        return;
    }
    let mut state = state_for_request(request, &rows);
    apply_bond_locks(&mut state, &locks);
    if let Some(block) =
        helper::render_state_block_with_locks(&state, &trackers, STATE_HEADER, &locks)
    {
        in_chat.push((
            STATE_DEPTH,
            ProviderNeutralMessage {
                role: MessageRole::System,
                parts: vec![ProviderContextPart::Text { text: block }],
            },
        ));
    }
}

/// The characters of a chat, whose switches the chat inherits.
pub(crate) fn character_ids(conversation: &lettuce_conversations::Conversation) -> Vec<String> {
    match &conversation.kind {
        lettuce_conversations::ConversationKind::Direct(details) => {
            vec![details.character.source_id.to_string()]
        }
        lettuce_conversations::ConversationKind::Group(_) => conversation
            .participants
            .iter()
            .filter_map(|participant| match participant.source {
                lettuce_conversations::ParticipantSource::Character(id) => Some(id.to_string()),
                _ => None,
            })
            .collect(),
    }
}
