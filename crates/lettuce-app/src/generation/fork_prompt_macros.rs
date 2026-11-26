//! Fork-only: SillyTavern macros over the assembled prompt.
//!
//! Runs after prompt entries, lorebook text and character fields are
//! rendered, on every non-history message in the order the model will read
//! them (the relative block first, then in-chat entries by final position),
//! so a `{{setvar}}` in an early block feeds a `{{getvar}}` in a later one.
//! Chat history is never evaluated, as in SillyTavern. Variables are loaded
//! from the fork store; a real generation saves the writes and logs the
//! dice, a preview does not.

use lettuce_context::fork_macros::{self, MacroEnv, MacroRun, Vars};
use lettuce_conversations::{MessageRole, ProviderContextPart, ProviderNeutralMessage};

/// One assembly's macro run, kept until the final prompt exists.
pub(crate) struct ForkMacroOutcome {
    run: Option<MacroRun>,
    conversation_id: String,
}

fn has_macros(message: &ProviderNeutralMessage) -> bool {
    message.parts.iter().any(|part| match part {
        ProviderContextPart::Text { text } => text.contains("{{"),
        _ => false,
    })
}

fn text_of(message: &ProviderNeutralMessage) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            ProviderContextPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Evaluates one message; `false` when it ended up empty (SillyTavern drops
/// empty prompt blocks, and empty text parts are not valid).
fn evaluate_message(run: &mut MacroRun, message: &mut ProviderNeutralMessage) -> bool {
    if !has_macros(message) {
        return true;
    }
    for part in &mut message.parts {
        if let ProviderContextPart::Text { text } = part
            && text.contains("{{")
        {
            *text = run.evaluate(text).trim().to_owned();
        }
    }
    message.parts.retain(|part| match part {
        ProviderContextPart::Text { text } => !text.is_empty(),
        _ => true,
    });
    !message.parts.is_empty()
}

/// The order in-chat entries land in (same key as `insert_in_chat_messages`:
/// position from the end of history, then rendered order).
fn in_chat_order(history_len: usize, in_chat: &[(u32, ProviderNeutralMessage)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..in_chat.len()).collect();
    order.sort_by_key(|&index| (history_len.saturating_sub(in_chat[index].0 as usize), index));
    order
}

fn env_for(
    history: &[ProviderNeutralMessage],
    roles: &[MessageRole],
    conversation_id: &str,
    user: &str,
    char: &str,
) -> MacroEnv {
    let last = |wanted: Option<MessageRole>| {
        history
            .iter()
            .zip(roles)
            .rev()
            .find(|(_, role)| match wanted {
                Some(wanted) => **role == wanted,
                None => matches!(role, MessageRole::User | MessageRole::Assistant),
            })
            .map(|(message, _)| text_of(message))
    };
    MacroEnv {
        user: Some(user.to_owned()),
        char: Some(char.to_owned()),
        last_message: last(None),
        last_user_message: last(Some(MessageRole::User)),
        last_char_message: last(Some(MessageRole::Assistant)),
        pick_seed: conversation_id.to_owned(),
    }
}

/// The hook before in-chat entries join history.
#[allow(clippy::too_many_arguments)]
pub(crate) fn evaluate(
    messages: &mut Vec<ProviderNeutralMessage>,
    in_chat: &mut Vec<(u32, ProviderNeutralMessage)>,
    history: &[ProviderNeutralMessage],
    roles: &[MessageRole],
    conversation_id: &str,
    user: &str,
    char: &str,
) -> ForkMacroOutcome {
    let conversation_id = conversation_id.to_owned();
    if !messages.iter().any(has_macros) && !in_chat.iter().any(|(_, m)| has_macros(m)) {
        return ForkMacroOutcome {
            run: None,
            conversation_id,
        };
    }
    let (vars, globals) = fork_macros::var_store().map_or_else(
        || (Vars::new(), Vars::new()),
        |store| (store.chat_vars(&conversation_id), store.global_vars()),
    );
    let mut run = MacroRun::new(
        env_for(history, roles, &conversation_id, user, char),
        vars,
        globals,
    );
    messages.retain_mut(|message| evaluate_message(&mut run, message));
    let mut keep = vec![true; in_chat.len()];
    for index in in_chat_order(history.len(), in_chat) {
        keep[index] = evaluate_message(&mut run, &mut in_chat[index].1);
    }
    let mut flags = keep.into_iter();
    in_chat.retain(|_| flags.next().unwrap_or(true));
    ForkMacroOutcome {
        run: Some(run),
        conversation_id,
    }
}

impl ForkMacroOutcome {
    /// The hook after the final prompt is built: saves variable writes and
    /// logs the dice under the prompt's fingerprint. `persist` is false for
    /// previews.
    pub(crate) fn commit(self, messages: &[ProviderNeutralMessage], persist: bool) {
        let Some(run) = self.run else {
            return;
        };
        if !persist {
            return;
        }
        let Some(store) = fork_macros::var_store() else {
            return;
        };
        store.apply_changes(
            &self.conversation_id,
            run.chat_changes(),
            run.global_changes(),
        );
        if !run.rolls().is_empty() {
            store.record_rolls(&self.conversation_id, &fingerprint(messages), run.rolls());
        }
    }

    #[cfg(test)]
    fn rolls(&self) -> &[fork_macros::DiceRoll] {
        self.run.as_ref().map_or(&[], MacroRun::rolls)
    }
}

/// The fingerprint of a prompt's text parts, in order.
pub(crate) fn fingerprint(messages: &[ProviderNeutralMessage]) -> String {
    fork_macros::context_fingerprint(messages.iter().flat_map(|message| {
        message.parts.iter().filter_map(|part| match part {
            ProviderContextPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }))
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

    fn texts(messages: &[ProviderNeutralMessage]) -> Vec<String> {
        messages.iter().map(text_of).collect()
    }

    #[test]
    fn in_chat_entries_run_in_final_order_and_history_is_left_alone() {
        let mut messages = vec![
            text(MessageRole::System, "{{setvar::a::1}}"),
            text(MessageRole::System, "start {{getvar::a}}"),
        ];
        // Rendered order: depth 0 reader first, depth 2 writer second; the
        // writer lands earlier in the prompt, so it runs first.
        let mut in_chat = vec![
            (0, text(MessageRole::System, "read {{getvar::b}}")),
            (
                2,
                text(MessageRole::System, "{{setvar::b::{{incvar::a}}}}write"),
            ),
        ];
        let history = vec![
            text(MessageRole::User, "{{getvar::a}} stays"),
            text(MessageRole::Assistant, "reply"),
            text(MessageRole::User, "latest"),
        ];
        let roles = history.iter().map(|m| m.role).collect::<Vec<_>>();
        let outcome = evaluate(
            &mut messages,
            &mut in_chat,
            &history,
            &roles,
            "chat",
            "Алекс",
            "Шарлотта",
        );
        // The reset block became empty and is dropped.
        assert_eq!(texts(&messages), ["start 1"]);
        assert_eq!(
            in_chat.iter().map(|(_, m)| text_of(m)).collect::<Vec<_>>(),
            ["read 2", "write"]
        );
        assert_eq!(texts(&history)[0], "{{getvar::a}} stays");
        assert!(outcome.rolls().is_empty());
    }

    #[test]
    fn chat_messages_and_names_reach_macros() {
        let mut messages = vec![text(
            MessageRole::System,
            "{{user}}/{{char}}: {{lastUserMessage}} | {{lastCharMessage}} | {{roll::1d1}}",
        )];
        let history = vec![
            text(MessageRole::Assistant, "hello"),
            text(MessageRole::User, "hi"),
        ];
        let roles = history.iter().map(|m| m.role).collect::<Vec<_>>();
        let outcome = evaluate(
            &mut messages,
            &mut Vec::new(),
            &history,
            &roles,
            "chat",
            "Алекс",
            "Шарлотта",
        );
        assert_eq!(texts(&messages), ["Алекс/Шарлотта: hi | hello | 1"]);
        assert_eq!(outcome.rolls().len(), 1);
    }

    #[test]
    fn nothing_to_do_without_macros() {
        let mut messages = vec![text(MessageRole::System, "plain")];
        let outcome = evaluate(&mut messages, &mut Vec::new(), &[], &[], "c", "u", "c");
        assert!(outcome.run.is_none());
        assert_eq!(texts(&messages), ["plain"]);
    }

    #[test]
    fn fingerprint_reads_only_text() {
        let messages = vec![text(MessageRole::System, "a"), text(MessageRole::User, "b")];
        assert_eq!(
            fingerprint(&messages),
            fork_macros::context_fingerprint(["a", "b"])
        );
    }
}

#[cfg(test)]
#[path = "fork_prompt_macros_e2e.rs"]
mod e2e;
