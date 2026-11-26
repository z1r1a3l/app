use super::*;
use crate::fork_macros::{MacroEnv, MacroRun, Vars};
use crate::fork_regex::{RegexScript, placement};

fn vars_of(text: &str) -> Vars {
    let adapted = adapt_text(text);
    let mut run = MacroRun::new(MacroEnv::default(), Vars::new(), Vars::new());
    let _ = run.evaluate(&adapted);
    run.chat_changes()
        .iter()
        .filter_map(|(name, value)| value.clone().map(|value| (name.clone(), value)))
        .collect()
}

fn get<'a>(vars: &'a Vars, name: &str) -> &'a str {
    vars.get(name).map(String::as_str).unwrap_or("")
}

#[test]
fn flags_store_the_bare_name_and_leave_surrounding_prose() {
    let text =
        "Mara holds a receiver.\n[[flag1: signal_audible]]\n[[flag2: battle_active]]\nKeep going.";
    let adapted = adapt_text(text);
    assert!(!adapted.contains("[[flag"), "{adapted}");
    assert!(adapted.contains("Mara holds a receiver."));
    assert!(adapted.contains("Keep going."));
    assert!(adapted.contains("{{setvar::flag1::signal_audible}}"));
    assert!(adapted.contains("{{setvar::flag2::battle_active}}"));
    let vars = vars_of(text);
    assert_eq!(get(&vars, "flag1"), "signal_audible");
    assert_eq!(get(&vars, "flag2"), "battle_active");
}

#[test]
fn hook_emits_condition_and_function_call() {
    let text = r#"[[flag1: hunted]]

[[hook1]]
condition: hunted = TRUE
call: hunted_scene
[[/hook1]]

<fn:hunted_scene>
  1. Render the chase.
  ⇥ <page:output>
</fn:hunted_scene>
"#;
    let adapted = adapt_text(text);
    assert!(!adapted.contains("[[hook1]]"), "{adapted}");
    assert!(!adapted.contains("[[flag1:"), "{adapted}");
    assert!(
        adapted.contains("<fn:hunted_scene>"),
        "function bodies stay in the card: {adapted}"
    );
    let vars = vars_of(text);
    assert_eq!(get(&vars, "flag1"), "hunted");
    assert_eq!(
        get(&vars, "hookCall1"),
        "condition: hunted = TRUE\n➤ <fn:hunted_scene>"
    );
}

#[test]
fn hook_accepts_a_page_hijack_field() {
    let text = "[[hook2]]\ncondition: duel_active = TRUE\npage: output\n[[/hook2]]";
    let vars = vars_of(text);
    assert_eq!(
        get(&vars, "hookCall2"),
        "condition: duel_active = TRUE\n⇥ <page:output>"
    );
}

#[test]
fn hook_strips_fn_wrappers_in_the_call_name() {
    let vars = vars_of("[[hook3]]\ncall: <fn:missing_signal_listen>\n[[/hook3]]");
    assert_eq!(get(&vars, "hookCall3"), "➤ <fn:missing_signal_listen>");
}

#[test]
fn arc_and_tracks_specs_store_the_inner_body() {
    // From AetherKit-SDK.md example 2 (shortened) and the public guide.
    let text = r#"
[[spec: arc]]
name: The Missing Signal
objective: identify the source of the signal
genre: mystery
focus: {{user}}
role: focus
duration: 2 days

ki (1):
  Mara brings a damaged receiver [C:Mara]
sho (1):
  The receiver plays a distress call [D:ki1]
[[/spec: arc]]

[[spec: tracks]]

  [[track: The Stalker]]
    objective: protect the clerk
    trigger: the camera repair
    chain:
      clerk asks for number → unknown texts → letters
    on_completion: merge into main arc
    on_failure: she's alone with it (permanent scar)
  [[/track]]

[[/spec: tracks]]
"#;
    let adapted = adapt_text(text);
    assert!(!adapted.contains("[[spec:"), "{adapted}");
    // Nested track tags stay inside the tracks variable, not as their own slot.
    let vars = vars_of(text);
    let arc = get(&vars, "pluginArcSpec");
    assert!(arc.contains("name: The Missing Signal"), "{arc}");
    assert!(
        arc.contains("focus: {{user}}") || arc.contains("focus: "),
        "{arc}"
    );
    // {{user}} is unknown without an env, so MacroRun leaves it (or empty).
    assert!(arc.contains("[C:Mara]"), "{arc}");
    assert!(!arc.contains("[[spec"), "{arc}");
    let tracks = get(&vars, "pluginTracksSpec");
    assert!(tracks.contains("[[track: The Stalker]]"), "{tracks}");
    assert!(
        tracks.contains("on_failure: she's alone with it"),
        "{tracks}"
    );
    assert!(tracks.contains("[[/track]]"), "{tracks}");
}

#[test]
fn state_stores_the_details_html() {
    let text = r#"[[state]]
<details>
  <summary>🧩 GAUNTLET STATE</summary>
  <li><b>Sanity:</b> 10</li>
</details>
[[/state]]"#;
    let vars = vars_of(text);
    let state = get(&vars, "pluginStateTemplate");
    assert!(state.contains("<details>"), "{state}");
    assert!(state.contains("🧩 GAUNTLET STATE"), "{state}");
    assert!(state.contains("<li><b>Sanity:</b> 10</li>"), "{state}");
    assert!(!state.contains("[[state]]"), "{state}");
}

#[test]
fn dice_keep_the_roll_macro_so_it_is_pre_rolled_at_setvar_time() {
    let text = r#"[[dice1]]
name: sanityRoll
roll: {{roll::1d1}}
[[/dice1]]

[[dice2]]
name: damageRoll
roll: {{roll::1d1+3}}
[[/dice2]]"#;
    let adapted = adapt_text(text);
    assert!(adapted.contains("{{roll::1d1}}"), "{adapted}");
    let vars = vars_of(text);
    let one = get(&vars, "pluginDice1");
    assert!(one.contains("name: sanityRoll"), "{one}");
    assert!(one.contains("roll: 1"), "{one}");
    let two = get(&vars, "pluginDice2");
    assert!(two.contains("name: damageRoll"), "{two}");
    assert!(two.contains("roll: 4"), "{two}");
}

#[test]
fn later_slot_writes_win_and_unknown_brackets_are_left_alone() {
    let text = "[[flag1: first]] keep [[mystery]] [[flag1: second]]";
    let adapted = adapt_text(text);
    assert!(adapted.contains("[[mystery]]"), "{adapted}");
    let vars = vars_of(text);
    assert_eq!(get(&vars, "flag1"), "second");
}

#[test]
fn unclosed_or_empty_blocks_are_left_as_written() {
    let text = "[[hook1]]\ncondition: x = TRUE\n[[flag3:]] [[spec: arc]] no closer";
    let adapted = adapt_text(text);
    assert_eq!(adapted, text);
    assert!(declarations(text).is_empty());
}

#[test]
fn card_prose_around_sdk_example_1_survives() {
    let text = r#"A horror card.

[[flag1: hunted]]

[[hook1]]
condition: hunted = TRUE
call: hunted_scene
[[/hook1]]

The woods are dark."#;
    let adapted = adapt_text(text);
    assert!(adapted.contains("A horror card."));
    assert!(adapted.contains("The woods are dark."));
    assert!(!adapted.contains("[[flag1"));
}

fn converter(find: &str, replace: &str) -> RegexScript {
    RegexScript {
        script_name: "AetherKit SDK".into(),
        find_regex: find.into(),
        replace_string: replace.into(),
        placement: vec![placement::WORLD_INFO],
        markdown_only: false,
        prompt_only: true,
        ..RegexScript::default()
    }
}

#[test]
fn conversion_script_detection_accepts_escaped_find_and_setvar_replace() {
    let escaped = converter(
        r"/\[\[flag([1-5]):\s*([^\]]+)\]\]/gi",
        "{{setvar::flag$1::$2}}",
    );
    assert!(is_conversion_script(&escaped));
    let by_replace = converter("something else", "{{setvar::pluginArcSpec::$1}}");
    assert!(is_conversion_script(&by_replace));
    let unrelated = converter("(<internal_states>)", "");
    assert!(!is_conversion_script(&unrelated));
}

#[test]
fn option_a_converters_win_over_the_built_in_adapter() {
    // A card pack that wraps the flag name rather than storing it bare: the
    // adapter must not also rewrite the shorthand.
    let script = converter(
        r"/\[\[flag([1-5]):\s*([^\]]+)\]\]/g",
        "{{setvar::flag$1::CARD:$2}}",
    );
    let text = "[[flag1: hunted]] and [[flag2: duel_active]]";
    let rewritten =
        rewrite_prompt_text(std::slice::from_ref(&script), text, &|v: &str| v.to_owned());
    assert!(
        rewritten.contains("{{setvar::flag1::CARD:hunted}}"),
        "{rewritten}"
    );
    assert!(
        rewritten.contains("{{setvar::flag2::CARD:duel_active}}"),
        "{rewritten}"
    );
    assert!(
        !rewritten.contains("{{setvar::flag1::hunted}}"),
        "{rewritten}"
    );
}

#[test]
fn without_a_converter_the_adapter_fills_vexa_slots() {
    let text = "[[flag1: hunted]]";
    let rewritten = rewrite_prompt_text(&[] as &[RegexScript], text, &|v: &str| v.to_owned());
    assert_eq!(rewritten, "{{setvar::flag1::hunted}}");
}

#[test]
fn official_vexa_regex_pack_is_not_a_converter() {
    let pack = include_str!("../../tests/fixtures/vexa/vexa-v1.0-regex.json");
    let scripts = crate::fork_regex::parse_regex_import(pack)
        .expect("vexa pack")
        .scripts;
    assert!(!scripts.is_empty());
    assert!(
        !has_conversion_script(&scripts),
        "the public v1.0 pack must not trip Option A"
    );
}
