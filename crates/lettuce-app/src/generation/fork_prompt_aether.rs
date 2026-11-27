//! Fork-only: AetherKit adapter at prompt assembly.
//!
//! Runs on non-history messages (preset entries, character fields, lorebook,
//! in-chat blocks) after history prompt-regex and before ST macros, so a
//! `{{setvar}}` produced from `[[flag]]` / `[[hook]]` / `[[spec]]` /
//! `[[state]]` / `[[dice]]` overrides Vexa's System Vars and is visible to
//! later `{{getvar}}` slots. Card-embedded conversion regex still wins.

use lettuce_context::fork_aether;
use lettuce_context::fork_regex::{self, RegexScopeKey, RegexScript};
use lettuce_conversations::{ProviderContextPart, ProviderNeutralMessage};

/// The hook: rewrite `messages` and `in_chat` in place.
pub(crate) fn apply_for_prompt(
    messages: &mut [ProviderNeutralMessage],
    in_chat: &mut [(u32, ProviderNeutralMessage)],
    prompt_id: Option<String>,
    character_ids: Vec<String>,
    user: &str,
    char: &str,
) {
    if !needs_work(messages, in_chat) {
        return;
    }
    let scripts = scripts_for(prompt_id, character_ids);
    // Do not run the full macro engine on replacements: a converter that
    // emits `{{setvar}}` would be evaluated immediately (and silently), then
    // System Vars would wipe the slot. Leave setvar for the later macro pass.
    let user = user.to_owned();
    let char = char.to_owned();
    let rewrite = |text: &str| {
        fork_aether::rewrite_prompt_text(&scripts, text, &|value: &str| {
            value.replace("{{user}}", &user).replace("{{char}}", &char)
        })
    };
    rewrite_messages(messages, &rewrite);
    for (_, message) in in_chat.iter_mut() {
        rewrite_one(message, &rewrite);
    }
}

fn scripts_for(prompt_id: Option<String>, character_ids: Vec<String>) -> Vec<RegexScript> {
    let Some(source) = fork_regex::regex_source() else {
        return Vec::new();
    };
    source.scripts_for(&RegexScopeKey {
        prompt_id,
        character_ids,
        conversation_id: None,
    })
}

fn needs_work(
    messages: &[ProviderNeutralMessage],
    in_chat: &[(u32, ProviderNeutralMessage)],
) -> bool {
    has_shorthand(messages)
        || in_chat
            .iter()
            .any(|(_, message)| has_shorthand(std::slice::from_ref(message)))
}

fn has_shorthand(messages: &[ProviderNeutralMessage]) -> bool {
    messages.iter().any(|message| {
        message.parts.iter().any(|part| match part {
            ProviderContextPart::Text { text } => text.contains("[["),
            _ => false,
        })
    })
}

fn rewrite_messages(messages: &mut [ProviderNeutralMessage], rewrite: &impl Fn(&str) -> String) {
    for message in messages {
        rewrite_one(message, rewrite);
    }
}

fn rewrite_one(message: &mut ProviderNeutralMessage, rewrite: &impl Fn(&str) -> String) {
    for part in &mut message.parts {
        if let ProviderContextPart::Text { text } = part
            && text.contains("[[")
        {
            *text = rewrite(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lettuce_context::fork_macros::{MacroEnv, MacroRun, Vars};
    use lettuce_context::fork_regex::{RegexScript, RegexScriptSource, placement};
    use lettuce_conversations::MessageRole;
    use std::sync::Arc;

    fn text(role: MessageRole, value: &str) -> ProviderNeutralMessage {
        ProviderNeutralMessage {
            role,
            parts: vec![ProviderContextPart::Text {
                text: value.to_owned(),
            }],
        }
    }

    fn joined(messages: &[ProviderNeutralMessage]) -> String {
        messages
            .iter()
            .flat_map(|message| &message.parts)
            .filter_map(|part| match part {
                ProviderContextPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn evaluate(messages: &mut [ProviderNeutralMessage]) -> Vars {
        let mut run = MacroRun::new(MacroEnv::default(), Vars::new(), Vars::new());
        for message in messages.iter_mut() {
            for part in &mut message.parts {
                if let ProviderContextPart::Text { text } = part
                    && text.contains("{{")
                {
                    *text = run.evaluate(text).trim().to_owned();
                }
            }
        }
        run.chat_changes()
            .iter()
            .filter_map(|(name, value)| value.clone().map(|value| (name.clone(), value)))
            .collect()
    }

    /// Minimal Vexa-like System Vars + Helper Functions / Dice Rolls slots.
    const SYSTEM_VARS: &str = "\
{{setvar::flag1::∅}}\
{{setvar::hookCall1::∅.}}\
{{setvar::pluginArcSpec::{{noop}}}}\
{{setvar::pluginTracksSpec::{{noop}}}}\
{{setvar::pluginStateTemplate::{{noop}}}}\
{{setvar::pluginDice1::{{noop}}}}";

    const GETVARS: &str = "\
FLAG:{{getvar::flag1}}\n\
HOOK:{{getvar::hookCall1}}\n\
ARC:{{getvar::pluginArcSpec}}\n\
TRACKS:{{getvar::pluginTracksSpec}}\n\
STATE:{{getvar::pluginStateTemplate}}\n\
DICE:{{getvar::pluginDice1}}";

    const CARD: &str = r#"Mara holds a receiver.

[[flag1: signal_audible]]

[[hook1]]
condition: signal_audible = TRUE
call: missing_signal_listen
[[/hook1]]

[[spec: arc]]
name: The Missing Signal
objective: identify the source of the signal
[[/spec: arc]]

[[spec: tracks]]
  [[track: The Rival]]
    objective: find the transmitter first
  [[/track]]
[[/spec: tracks]]

[[state]]
<details>
  <summary>🧩 SIGNAL STATE</summary>
  <li><b>Damage:</b> 2</li>
</details>
[[/state]]

[[dice1]]
name: sanityRoll
roll: {{roll::1d1}}
[[/dice1]]

<fn:missing_signal_listen>
  Describe the signal through sounds {{user}} can hear.
  ⇤
</fn:missing_signal_listen>
"#;

    #[test]
    fn vexa_slots_fill_from_a_sample_card_and_shorthand_is_stripped() {
        let mut messages = vec![
            text(MessageRole::System, SYSTEM_VARS),
            text(MessageRole::System, CARD),
            text(
                MessageRole::System,
                "lore: [[flag1: from_lore]] should lose to later? wait this is after card in this vec...",
            ),
            text(MessageRole::System, GETVARS),
        ];
        // Lorebook-shaped message between the card and getvar, earlier than
        // getvar, later than the card — last write to flag1 wins.
        apply_for_prompt(
            &mut messages,
            &mut [],
            None,
            Vec::new(),
            "Traveller",
            "Mara",
        );
        let vars = evaluate(&mut messages);
        let text = joined(&messages);
        assert!(!text.contains("[[flag1:"), "{text}");
        assert!(!text.contains("[[hook1]]"), "{text}");
        assert!(!text.contains("[[spec:"), "{text}");
        assert!(!text.contains("[[state]]"), "{text}");
        assert!(!text.contains("[[dice1]]"), "{text}");
        assert!(
            text.contains("<fn:missing_signal_listen>"),
            "function body stays: {text}"
        );
        assert!(text.contains("Mara holds a receiver."), "{text}");
        // The later lore message overrides flag1.
        assert_eq!(vars["flag1"], "from_lore");
        assert!(
            text.contains("FLAG:from_lore"),
            "getvar after System Vars wipe sees the card/lore write: {text}"
        );
        assert!(
            text.contains("HOOK:condition: signal_audible = TRUE\n➤ <fn:missing_signal_listen>"),
            "{text}"
        );
        assert!(text.contains("ARC:name: The Missing Signal"), "{text}");
        assert!(text.contains("[[track: The Rival]]"), "{text}");
        assert!(text.contains("🧩 SIGNAL STATE"), "{text}");
        assert!(text.contains("<details>"), "{text}");
        assert!(text.contains("DICE:name: sanityRoll"), "{text}");
        assert!(text.contains("roll: 1"), "{text}");
        // System Vars emptied the slots first; the adapter overwrote them.
        assert_ne!(vars["hookCall1"], "∅.");
    }

    struct Pack(Vec<RegexScript>);

    impl RegexScriptSource for Pack {
        fn scripts_for(&self, _key: &RegexScopeKey) -> Vec<RegexScript> {
            self.0.clone()
        }
    }

    #[test]
    fn card_conversion_regex_wins_over_the_adapter() {
        let script = RegexScript {
            script_name: "SDK regex".into(),
            find_regex: r"/\[\[flag([1-5]):\s*([^\]]+)\]\]/g".into(),
            replace_string: "{{setvar::flag$1::CARD:$2}}".into(),
            placement: vec![placement::WORLD_INFO],
            markdown_only: false,
            prompt_only: true,
            ..RegexScript::default()
        };
        struct ResetSource;
        impl Drop for ResetSource {
            fn drop(&mut self) {
                fork_regex::clear_regex_source();
            }
        }
        let _reset = ResetSource;
        fork_regex::set_regex_source(Arc::new(Pack(vec![script])));
        let mut messages = vec![
            text(MessageRole::System, SYSTEM_VARS),
            text(MessageRole::System, "[[flag1: hunted]]"),
            text(MessageRole::System, "FLAG:{{getvar::flag1}}"),
        ];
        apply_for_prompt(
            &mut messages,
            &mut [],
            None,
            vec!["char-1".into()],
            "Traveller",
            "Mara",
        );
        evaluate(&mut messages);
        let text = joined(&messages);
        assert!(text.contains("FLAG:CARD:hunted"), "{text}");
        assert!(
            !text.contains("FLAG:hunted\n") && !text.ends_with("FLAG:hunted"),
            "{text}"
        );
    }
}

#[cfg(test)]
#[path = "fork_prompt_aether_e2e.rs"]
mod e2e;
