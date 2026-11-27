//! Fork-only: append folded chapter recall to the memory summary in the prompt.

use lettuce_conversations::TimelineItem;

const MAX_MESSAGES: usize = 12;
const MAX_CHARS: usize = 4000;

pub(crate) fn recent_scene_text(items: &[TimelineItem]) -> String {
    let mut parts = Vec::new();
    for item in items.iter().rev().take(MAX_MESSAGES).rev() {
        let text = crate::generation::fork_helper_run::item_text(item);
        if !text.trim().is_empty() {
            parts.push(text);
        }
    }
    let mut out = parts.join("\n");
    if out.chars().count() > MAX_CHARS {
        out = out
            .chars()
            .rev()
            .take(MAX_CHARS)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
    }
    out
}

/// Appends recalled chapters when the fork source is on.
pub(crate) fn with_recalled_chapters(
    summary: Option<String>,
    space_id: impl ToString,
    conversation_id: impl ToString,
    branch_id: impl ToString,
    items: &[TimelineItem],
) -> Option<String> {
    lettuce_memory::append_recalled_chapters(
        summary,
        &space_id.to_string(),
        &conversation_id.to_string(),
        &branch_id.to_string(),
        &recent_scene_text(items),
    )
}
