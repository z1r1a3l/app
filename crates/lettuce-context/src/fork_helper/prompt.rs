//! The helper's default prompt and the input it reads each run.

use std::fmt::Write as _;

use super::model::{Tracker, Trackers};

/// How many unrevealed truths the Notebook keeps at most.
pub const MAX_LIVE_TRUTHS: usize = 3;

/// The default system prompt (user-editable). Telegraphic English; values
/// in Russian. `{{user}}` and `{{char}}` are filled in.
pub const DEFAULT_PROMPT: &str = "\
Role: state tracker for ongoing roleplay. Not a writer. Never continue story. Never explain.
Input: current state, recent story, latest reply.
Task: write ONLY changes caused by latest exchange. One change per line. Nothing else.
Language: all values in Russian. Section tags and field names in English, as shown.
NSFW/NSFL allowed; all descriptions explicit. No refusals, no censoring, no moral comments.
{{user}} = player. Never invent or change {{user}}'s backstory, thoughts, motives. Track only visible facts about {{user}} (outfit, location, items).

Formats:
[Scene] Time: <old> -> <new>          fields: Time, Place, Weather, Situation
[Characters] <Name> · Outfit: <old> -> <new>          fields: Outfit, Location, Pose, Condition
[Characters] - <Name>          (left story for good)
[Bonds] <Name> · Attitude: <old> -> <new>          fields: Attitude, Trust 0-100, Affection 0-100 (towards {{user}})
[Inventory] + <Owner>: <item>, <item>
[Inventory] - <Owner>: <item>
[NPC] <Name> · Role: <value> · Look: <value>          fields: Role, Look, Status, Attitude; NPC = anyone except {{char}} and {{user}}
[Notebook] + T<n>: <hidden truth> | hidden from: <names>
[Notebook] Clue T<n>: <exact phrase from latest reply that hints at it>
[Notebook] Reveal T<n>

Rules:
- Only trackers listed under Enabled trackers.
- Skip unchanged fields. Short values, max ~12 words.
- Several fields of one name may share a line, separated by ·.
- Nothing changed -> write: No changes
- Notebook: hidden truths about world and NPCs only, never about {{user}}. Keep 2-3 unrevealed truths; when fewer than 2, add new ones with next free T<n>. Truths come from setting, card, story; nothing in story gave them away yet.
- Clue only when latest reply text really shows it; quote its words. Reveal when truth came out in latest reply.";

/// One line of the story the helper reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperLine {
    pub speaker: String,
    pub text: String,
}

/// Everything one run reads besides the prompt.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HelperInput {
    pub character_name: String,
    /// The character card (description), shortened.
    pub character_card: String,
    pub user_name: String,
    /// The state before the latest reply (`render_state_block`).
    pub state: Option<String>,
    pub earlier: Vec<HelperLine>,
    pub latest: Vec<HelperLine>,
    pub trackers: Trackers,
    pub live_truths: usize,
    /// Bonds the user froze; the helper must not change them.
    pub locked_bonds: Vec<String>,
}

const MAX_CARD_CHARS: usize = 4000;

fn shorten(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out = text.chars().take(max).collect::<String>();
    out.push('…');
    out
}

/// The prompt with `{{user}}`/`{{char}}` filled in.
#[must_use]
pub fn helper_system_prompt(prompt: &str, user_name: &str, character_name: &str) -> String {
    let prompt = if prompt.trim().is_empty() {
        DEFAULT_PROMPT
    } else {
        prompt
    };
    prompt
        .replace("{{user}}", user_name)
        .replace("{{User}}", user_name)
        .replace("{{char}}", character_name)
        .replace("{{Char}}", character_name)
}

fn lines(out: &mut String, tag: &str, lines: &[HelperLine]) {
    if lines.is_empty() {
        return;
    }
    let _ = writeln!(out, "<{tag}>");
    for line in lines {
        let _ = writeln!(out, "{}: {}", line.speaker, line.text.trim());
    }
    let _ = writeln!(out, "</{tag}>\n");
}

/// The user message of one run.
#[must_use]
pub fn helper_user_message(input: &HelperInput) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Main character: {}. Player: {}.",
        input.character_name, input.user_name
    );
    if !input.character_card.trim().is_empty() {
        let _ = writeln!(
            out,
            "<card>\n{}\n</card>",
            shorten(&input.character_card, MAX_CARD_CHARS)
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "<current_state>\n{}\n</current_state>\n",
        input
            .state
            .as_deref()
            .unwrap_or("(empty: write the starting values seen so far)")
    );
    lines(&mut out, "recent_story", &input.earlier);
    lines(&mut out, "latest_exchange", &input.latest);
    let enabled = input
        .trackers
        .list()
        .into_iter()
        .map(Tracker::tag)
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "Enabled trackers: {enabled}");
    if input.trackers.enabled(Tracker::Notebook) {
        let _ = writeln!(
            out,
            "Notebook: {} unrevealed truth(s) now (keep 2-{}).",
            input.live_truths, MAX_LIVE_TRUTHS
        );
    }
    if !input.locked_bonds.is_empty() {
        let _ = writeln!(
            out,
            "Locked bonds (do not change or remove): {}.",
            input.locked_bonds.join(", ")
        );
    }
    let _ = write!(out, "Write the change lines now.");
    out
}
