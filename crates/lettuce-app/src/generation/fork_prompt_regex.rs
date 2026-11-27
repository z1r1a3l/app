//! Fork-only: prompt-side regex scripts over chat history.
//!
//! Runs SillyTavern "Alter Outgoing Prompt" scripts on each history message
//! before it reaches the provider. Depth counts from the newest message
//! (depth 0), as in SillyTavern. The chat's stored text is never changed.

use lettuce_context::fork_regex::{self, BasicMacroValues, RegexScopeKey, placement};
use lettuce_conversations::{MessageRole, ProviderContextPart, ProviderNeutralMessage};

/// The prompt hook: remembers the chat's scope for memory work, then
/// rewrites `transcript`. `roles` holds each message's role as stored
/// (before group or swap-places role mapping), one per message.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_for_chat(
    transcript: &mut [ProviderNeutralMessage],
    roles: &[MessageRole],
    conversation_id: &str,
    prompt_id: Option<String>,
    character_ids: Vec<String>,
    user: &str,
    char: &str,
) {
    let key = RegexScopeKey {
        prompt_id,
        character_ids,
        conversation_id: Some(conversation_id.to_owned()),
    };
    let mut macros = BasicMacroValues {
        user: user.to_owned(),
        char: char.to_owned(),
        ..Default::default()
    };
    // `{{getvar}}` / `{{getglobalvar}}` in replacements read the stored
    // variables (as they stood before this turn's macros ran).
    if let Some(store) = lettuce_context::fork_macros::var_store() {
        macros.vars = store.chat_vars(conversation_id).into_iter().collect();
        macros.global_vars = store.global_vars().into_iter().collect();
        macros.custom = store.chat_macros(conversation_id).into_iter().collect();
    }
    fork_regex::remember_scope(conversation_id, &key, &macros);
    let Some(rewrite) = fork_regex::prompt_regex_for(&key, macros) else {
        return;
    };
    rewrite_transcript(transcript, roles, &rewrite);
}

fn placement_for(role: MessageRole) -> Option<u8> {
    match role {
        MessageRole::User => Some(placement::USER_INPUT),
        MessageRole::Assistant => Some(placement::AI_OUTPUT),
        _ => None,
    }
}

fn depth_at(count: usize, index: usize) -> i64 {
    i64::try_from(count - index - 1).unwrap_or(i64::MAX)
}

fn rewrite_transcript(
    transcript: &mut [ProviderNeutralMessage],
    roles: &[MessageRole],
    rewrite: &impl Fn(&str, u8, i64) -> String,
) {
    let count = transcript.len();
    for (index, (message, role)) in transcript.iter_mut().zip(roles).enumerate() {
        let Some(placement) = placement_for(*role) else {
            continue;
        };
        let depth = depth_at(count, index);
        for part in &mut message.parts {
            if let ProviderContextPart::Text { text } = part {
                *text = rewrite(text, placement, depth);
            }
        }
    }
}

/// Cleans memory, summary and Eidos source texts (oldest first) the way the
/// chat's prompt saw them, using the scope its last prompt assembly used.
/// Without one (first run after a restart) only global scripts apply.
pub(crate) fn clean_memory_sources(
    conversation_id: &str,
    texts: &mut [(MessageRole, &mut String)],
) {
    let (mut key, macros) = fork_regex::remembered_scope(conversation_id).unwrap_or_default();
    key.conversation_id = Some(conversation_id.to_owned());
    let Some(rewrite) = fork_regex::prompt_regex_for(&key, macros) else {
        return;
    };
    let count = texts.len();
    for (index, (role, text)) in texts.iter_mut().enumerate() {
        let Some(placement) = placement_for(*role) else {
            continue;
        };
        **text = rewrite(text, placement, depth_at(count, index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(role: MessageRole, value: &str) -> ProviderNeutralMessage {
        ProviderNeutralMessage {
            role,
            parts: vec![ProviderContextPart::Text {
                text: value.to_owned(),
            }],
        }
    }

    fn texts(transcript: &[ProviderNeutralMessage]) -> Vec<String> {
        transcript
            .iter()
            .map(|m| match &m.parts[0] {
                ProviderContextPart::Text { text } => text.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn depth_counts_from_the_newest_message_and_placement_follows_the_stored_role() {
        let mut transcript = vec![
            text(MessageRole::User, "a"),
            text(MessageRole::Assistant, "b"),
            text(MessageRole::System, "c"),
            text(MessageRole::User, "d"),
        ];
        let roles = transcript.iter().map(|m| m.role).collect::<Vec<_>>();
        rewrite_transcript(&mut transcript, &roles, &|text, placement, depth| {
            format!("{text}:{placement}:{depth}")
        });
        assert_eq!(texts(&transcript), ["a:1:3", "b:2:2", "c", "d:1:0"]);
    }

    #[test]
    fn vexa_prompt_scripts_strip_old_state_blocks() {
        let pack =
            include_str!("../../../lettuce-context/tests/fixtures/vexa/vexa-v1.0-regex.json");
        let reply = include_str!("../../../lettuce-context/tests/fixtures/vexa/reply-full.txt");
        let scripts = fork_regex::parse_regex_import(pack)
            .expect("pack")
            .scripts
            .into_iter()
            .filter(|s| s.applies_to(fork_regex::RegexTarget::Prompt))
            .collect::<Vec<_>>();
        let rewrite = |text: &str, placement: u8, depth: i64| {
            fork_regex::apply_regex_scripts(
                &scripts,
                text,
                fork_regex::RegexApplyContext {
                    placement,
                    target: fork_regex::RegexTarget::Prompt,
                    depth: Some(depth),
                },
                &|value: &str| value.to_owned(),
            )
        };
        let mut transcript = vec![
            text(MessageRole::Assistant, reply),
            text(MessageRole::User, "ok"),
            text(MessageRole::Assistant, reply),
            text(MessageRole::User, "ok"),
            text(MessageRole::Assistant, reply),
        ];
        let roles = transcript.iter().map(|m| m.role).collect::<Vec<_>>();
        rewrite_transcript(&mut transcript, &roles, &rewrite);
        let out = texts(&transcript);
        assert!(out[4].contains("<internal_states>"));
        assert!(out[2].contains("<internal_states>"));
        assert!(!out[0].contains("<internal_states>"));
        assert_eq!(out[1], "ok");
    }
}
