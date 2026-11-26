use super::*;

fn run() -> MacroRun {
    MacroRun::new(
        MacroEnv {
            user: Some("Алекс".into()),
            char: Some("Шарлотта".into()),
            last_message: Some("last".into()),
            last_user_message: Some("hi there".into()),
            last_char_message: Some("hello".into()),
            pick_seed: "chat-1".into(),
        },
        Vars::new(),
        Vars::new(),
    )
    .with_seed(7)
}

fn eval(text: &str) -> String {
    run().evaluate(text)
}

#[test]
fn plain_text_is_untouched() {
    assert_eq!(eval("no macros here { } }}"), "no macros here { } }}");
    assert_eq!(eval(""), "");
}

#[test]
fn comments_noop_newline_space() {
    assert_eq!(eval("a{{// note}}b"), "ab");
    assert_eq!(eval("a{{//multi\nline {{user}} }}b"), "ab");
    assert_eq!(eval("a{{noop}}b{{newline}}c{{space}}d"), "ab\nc d");
}

#[test]
fn trim_eats_surrounding_line_breaks_only() {
    assert_eq!(eval("x\n\n{{trim}}\n\ny"), "xy");
    assert_eq!(eval("x \n{{trim}} y"), "x  y");
    assert_eq!(eval("x\r\n{{trim}}\r\ny"), "xy");
    assert_eq!(eval("{{// head}}{{trim}}\n  {{setvar::a::1}}\n"), "  \n");
    assert_eq!(eval("a{{TRIM}}\nb"), "ab");
}

#[test]
fn names_and_unknown_macros() {
    assert_eq!(eval("{{user}} и {{char}}"), "Алекс и Шарлотта");
    assert_eq!(eval("{{ User }}"), "Алекс");
    assert_eq!(eval("{{unknown::x}}"), "{{unknown::x}}");
    assert_eq!(
        eval("{{char.desc}} {{persona.name}}"),
        "{{char.desc}} {{persona.name}}"
    );
    assert_eq!(eval("{{#if scene}}x{{/if}}"), "{{#if scene}}x{{/if}}");
    assert_eq!(eval("{{unknown::{{user}}}}"), "{{unknown::Алекс}}");
    let mut hostless = MacroRun::new(MacroEnv::default(), Vars::new(), Vars::new());
    assert_eq!(hostless.evaluate("{{user}}"), "{{user}}");
}

#[test]
fn unbalanced_braces_stay_text() {
    assert_eq!(eval("{{user"), "{{user");
    assert_eq!(eval("a {{ b {{user}}"), "a {{ b Алекс");
    assert_eq!(eval("{{user}}}}"), "Алекс}}");
    assert_eq!(eval("{{{user}}}"), "{{{user}}}");
}

#[test]
fn variables_follow_document_order() {
    assert_eq!(eval("{{setvar::a::1}}{{getvar::a}}"), "1");
    assert_eq!(eval("{{getvar::a}}{{setvar::a::1}}"), "");
    assert_eq!(eval("{{setvar::a:: spaced \n}}[{{getvar::a}}]"), "[spaced]");
    assert_eq!(eval("{{setvar::a::x::y}}{{getvar::a}}"), "x::y");
    assert_eq!(
        eval("{{setvar::a::{{noop}}}}[{{getvar::a}}]{{hasvar::a}}"),
        "[]true"
    );
    assert_eq!(eval("{{hasvar::nope}}{{varexists::nope}}"), "falsefalse");
    assert_eq!(
        eval("{{setvar::a::1}}{{deletevar::a}}{{hasvar::a}}"),
        "false"
    );
}

#[test]
fn numeric_variables_behave_like_javascript() {
    assert_eq!(eval("{{incvar::n}}{{incvar::n}}{{decvar::m}}"), "12-1");
    assert_eq!(
        eval("{{setvar::n::5}}{{addvar::n::2.5}}{{getvar::n}}"),
        "7.5"
    );
    assert_eq!(eval("{{setvar::n:: 05 }}{{getvar::n}}"), "5");
    assert_eq!(
        eval("{{setvar::s::abc}}{{addvar::s::def}}{{getvar::s}}"),
        "abcdef"
    );
    assert_eq!(eval("{{setvar::s::5}}{{addvar::s::x}}{{getvar::s}}"), "5x");
    assert_eq!(eval("{{addvar::fresh::x}}{{getvar::fresh}}"), "x");
    assert_eq!(eval("{{setvar::s::abc}}{{incvar::s}}"), "abc1");
    assert_eq!(
        eval("{{setvar::l::[\"a\"]}}{{addvar::l::b}}{{getvar::l}}"),
        r#"["a","b"]"#
    );
    assert_eq!(
        eval("{{setvar::f::0.1}}{{addvar::f::0.2}}{{getvar::f}}"),
        "0.30000000000000004"
    );
}

#[test]
fn global_variables_are_separate() {
    let mut run = run();
    assert_eq!(
        run.evaluate("{{setglobalvar::theme::Sola}}{{getvar::theme}}|{{getglobalvar::theme}}"),
        "|Sola"
    );
    assert_eq!(
        run.evaluate("{{incglobalvar::g}}{{addglobalvar::g::4}}{{getglobalvar::g}}"),
        "15"
    );
    assert_eq!(
        run.evaluate("{{hasglobalvar::theme}}{{flushglobalvar::theme}}{{globalvarexists::theme}}"),
        "truefalse"
    );
    assert_eq!(run.globals().get("g").map(String::as_str), Some("5"));
    assert_eq!(run.global_changes().get("theme"), Some(&None));
    assert_eq!(run.global_changes().get("g"), Some(&Some("5".to_owned())));
    assert!(run.chat_changes().is_empty());
}

#[test]
fn variables_persist_across_texts_of_one_run() {
    let mut run = run();
    assert_eq!(run.evaluate("{{setvar::mood::calm}}"), "");
    assert_eq!(run.evaluate("Mood: {{getvar::mood}}"), "Mood: calm");
    assert_eq!(run.vars().get("mood").map(String::as_str), Some("calm"));
}

#[test]
fn starting_variables_are_read() {
    let mut vars = Vars::new();
    vars.insert("hp".into(), "10".into());
    let mut globals = Vars::new();
    globals.insert("frankenSIM.mainTheme".into(), "Sola".into());
    let mut run = MacroRun::new(MacroEnv::default(), vars, globals);
    assert_eq!(
        run.evaluate("{{decvar::hp}} {{getglobalvar::frankenSIM.mainTheme}}"),
        "9 Sola"
    );
}

#[test]
fn nesting_resolves_inside_out() {
    assert_eq!(eval("{{setvar::who::{{user}}}}{{getvar::who}}"), "Алекс");
    assert_eq!(
        eval("{{setvar::k::who}}{{setvar::who::X}}{{getvar::{{getvar::k}}}}"),
        "X"
    );
    assert_eq!(eval("[{{random::{{user}}}}]"), "[Алекс]");
    assert_eq!(eval("{{reverse::{{char}}}}"), "аттолраШ");
}

#[test]
fn dice_roll_and_are_logged() {
    let mut run = run();
    let out = run.evaluate("{{roll::2d6+3}} {{roll:1d20}} {{roll 4}} {{roll::d8-1}}");
    let rolls = run.rolls();
    assert_eq!(rolls.len(), 4);
    let totals: Vec<String> = rolls.iter().map(|r| r.total.to_string()).collect();
    assert_eq!(out, totals.join(" "));
    assert_eq!(rolls[0].formula, "2d6+3");
    assert_eq!(rolls[0].dice.len(), 2);
    assert!(rolls[0].dice.iter().all(|d| (1..=6).contains(d)));
    assert_eq!(rolls[0].total, rolls[0].dice.iter().sum::<u64>() as i64 + 3);
    assert!((1..=20).contains(&rolls[1].total));
    assert!((1..=4).contains(&rolls[2].total));
    assert_eq!(rolls[3].modifier, -1);
    assert_eq!(eval("{{roll::banana}}{{roll::0d6}}"), "");
}

#[test]
fn rolls_rerun_every_generation() {
    let faces = (0..20)
        .map(|seed| {
            let mut run =
                MacroRun::new(MacroEnv::default(), Vars::new(), Vars::new()).with_seed(seed);
            run.evaluate("{{roll::1d1000}}")
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert!(faces.len() > 10);
    // Two fresh runs (no fixed seed) differ almost surely.
    let a = MacroRun::new(MacroEnv::default(), Vars::new(), Vars::new())
        .evaluate("{{roll::1d1000000}}");
    let b = MacroRun::new(MacroEnv::default(), Vars::new(), Vars::new())
        .evaluate("{{roll::1d1000000}}");
    assert_ne!(a, b);
}

#[test]
fn random_forms() {
    for _ in 0..20 {
        let mut r = MacroRun::new(MacroEnv::default(), Vars::new(), Vars::new());
        let a = r.evaluate("{{random::a::b::c}}");
        assert!(["a", "b", "c"].contains(&a.as_str()));
        let b = r.evaluate("{{random: x , y }}");
        assert!(["x", "y"].contains(&b.as_str()), "{b}");
        let c = r.evaluate("{{random:1\\,5,2}}");
        assert!(["1,5", "2"].contains(&c.as_str()), "{c}");
    }
    assert_eq!(eval("{{random}}"), "");
}

#[test]
fn pick_is_stable_per_chat() {
    let first = eval("{{pick::a::b::c::d::e::f::g}} {{pick::a::b::c::d::e::f::g}}");
    for _ in 0..5 {
        assert_eq!(
            eval("{{pick::a::b::c::d::e::f::g}} {{pick::a::b::c::d::e::f::g}}"),
            first
        );
    }
    let other_chats = (0..20)
        .map(|n| {
            MacroRun::new(
                MacroEnv {
                    pick_seed: format!("chat-{n}"),
                    ..Default::default()
                },
                Vars::new(),
                Vars::new(),
            )
            .evaluate("{{pick::a::b::c::d::e::f::g}}")
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert!(other_chats.len() > 1);
}

#[test]
fn chat_messages() {
    assert_eq!(
        eval("{{input}}|{{lastUserMessage}}|{{lastCharMessage}}|{{lastMessage}}"),
        "hi there|hi there|hello|last"
    );
}

#[test]
fn huge_values_are_capped() {
    let big = "x".repeat(MAX_VALUE_BYTES + 10);
    let mut run = run();
    run.evaluate(&format!("{{{{setvar::big::{big}}}}}"));
    assert_eq!(run.vars()["big"].len(), MAX_VALUE_BYTES);
}

#[test]
fn deep_nesting_does_not_overflow() {
    let deep = format!("{}x{}", "{{".repeat(5000), "}}".repeat(5000));
    let out = eval(&deep);
    assert!(out.contains('x'));
}

#[test]
fn fingerprint_is_stable_and_order_sensitive() {
    let a = context_fingerprint(["a", "b"]);
    assert_eq!(a, context_fingerprint(["a", "b"]));
    assert_ne!(a, context_fingerprint(["b", "a"]));
    assert_ne!(a, context_fingerprint(["ab"]));
    assert_eq!(a.len(), 16);
}

#[derive(serde::Deserialize)]
struct FixturePrompt {
    name: String,
    content: String,
}

/// Vexa's own blocks, in the order its preset sends them: the system vars
/// reset, theme globals, World Sim and D&D Sim set their roll variables,
/// and the in-chat "Dice Rolls" block (depth 0, last) reads them.
#[test]
fn vexa_blocks_set_and_read_their_variables() {
    let prompts: Vec<FixturePrompt> =
        serde_json::from_str(include_str!("../../tests/fixtures/vexa/macro-prompts.json"))
            .expect("fixture");
    let by_name = |name: &str| {
        prompts
            .iter()
            .find(|p| p.name.starts_with(name))
            .unwrap_or_else(|| panic!("{name}"))
    };
    let mut run = run();
    let system = run.evaluate(&by_name("DO NOT DISABLE").content);
    assert!(!system.contains("{{"), "{system}");
    assert_eq!(run.vars().get("d&dRolls").map(String::as_str), Some(""));
    assert_eq!(run.vars().get("flag1").map(String::as_str), Some("∅"));
    let themes = run.evaluate(&by_name("Internal States Themes").content);
    assert!(!themes.contains("{{"));
    assert_eq!(run.globals()["frankenSIM.mainTheme"], "Sola");
    assert_eq!(run.globals()["frankenSIM.mainStyle"], "Afterglow");
    run.evaluate(&by_name("World Sim").content);
    let dnd = run.evaluate(&by_name("D&D Sim").content);
    assert!(!dnd.contains("{{"));
    assert_eq!(run.vars()["userDiceCall"], "➤ <fn:user_dice>");
    let dice = run.evaluate(&by_name("Dice Rolls").content);
    assert!(dice.contains("D&D Sim Rolls:"), "{dice}");
    assert!(!dice.contains("{{"), "{dice}");
    // 5 D&D rolls plus World Sim's.
    assert!(run.rolls().len() >= 6, "{}", run.rolls().len());
    let user_roll = run.rolls()[run.rolls().len() - 5].total.to_string();
    assert!(
        dice.contains(&format!("User Rolls (rolled fresh each turn): {user_roll}")),
        "{dice}"
    );
}

/// The full official preset, when present: every enabled block evaluates
/// with no variable or dice macro left behind.
#[test]
fn official_vexa_preset_evaluates_cleanly() {
    let Ok(dir) = std::env::var("VEXA_OFFICIAL_DIR") else {
        return;
    };
    let raw = std::fs::read_to_string(format!("{dir}/ST.+.Tavo.Vexa.v1.0.json")).expect("preset");
    let preset: serde_json::Value = serde_json::from_str(&raw).expect("json");
    let mut run = run();
    let mut evaluated = 0;
    for prompt in preset["prompts"].as_array().expect("prompts") {
        let content = prompt["content"].as_str().unwrap_or_default();
        let out = run.evaluate(content);
        evaluated += 1;
        for name in [
            "{{setvar",
            "{{getvar",
            "{{roll",
            "{{random",
            "{{trim",
            "{{noop",
            "{{//",
            "{{setglobalvar",
        ] {
            assert!(
                !out.to_lowercase().contains(name),
                "{name} left in {}",
                prompt["name"]
            );
        }
    }
    assert!(evaluated > 50);
}
