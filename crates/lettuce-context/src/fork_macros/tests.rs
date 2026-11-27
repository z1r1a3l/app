use super::*;

fn run() -> MacroRun {
    MacroRun::new(env(), Vars::new(), Vars::new()).with_seed(7)
}

fn env() -> MacroEnv {
    MacroEnv {
        user: Some("Алекс".into()),
        char: Some("Шарлотта".into()),
        last_message: Some("last".into()),
        last_user_message: Some("hi there".into()),
        last_char_message: Some("hello".into()),
        pick_seed: "chat-1".into(),
        custom: [("town".to_owned(), "Ривертон".to_owned())].into_iter().collect(),
    }
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

// ---- {{if}}, scoped blocks, shorthand, varkey (SillyTavern 1.13+) ----

fn run_with(vars: &[(&str, &str)], globals: &[(&str, &str)]) -> MacroRun {
    let map = |list: &[(&str, &str)]| -> Vars {
        list.iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    };
    MacroRun::new(env(), map(vars), map(globals)).with_seed(7)
}

fn eval_with(vars: &[(&str, &str)], text: &str) -> (String, MacroRun) {
    let mut run = run_with(vars, &[]);
    let out = run.evaluate(text);
    (out, run)
}

#[test]
fn if_blocks_pick_a_branch() {
    assert_eq!(eval("{{if 1}}yes{{/if}}"), "yes");
    assert_eq!(eval("{{if 0}}yes{{/if}}"), "");
    assert_eq!(eval("{{if off}}yes{{else}}no{{/if}}"), "no");
    assert_eq!(eval("{{if FALSE}}yes{{else}}no{{/if}}"), "no");
    assert_eq!(eval("{{if anything}}yes{{else}}no{{/if}}"), "yes");
    // Names only the app knows stay for it.
    assert_eq!(
        eval("{{if description}}d{{/if}}"),
        "{{if description}}d{{/if}}"
    );
    assert_eq!(eval("{{#if lorebook}}l{{/if}}"), "{{#if lorebook}}l{{/if}}");
    assert_eq!(eval("{{if !persona::p}}"), "{{if !persona::p}}");
    assert_eq!(eval("{{if !0}}yes{{else}}no{{/if}}"), "yes");
    assert_eq!(eval("{{if ! 1}}yes{{else}}no{{/if}}"), "no");
    // Inline form.
    assert_eq!(eval("{{if 1::inline}}"), "inline");
    assert_eq!(eval("{{if 0::inline}}"), "");
    // A missing closer leaves the opener as written.
    assert_eq!(eval("{{if 1}}x"), "{{if 1}}x");
    assert_eq!(eval("x{{/if}}"), "x{{/if}}");
    // Upstream-style `{{#if}}` that the app did not resolve: `#` keeps
    // whitespace.
    assert_eq!(eval("{{#if 1}}  a  {{/if}}|"), "  a  |");
    assert_eq!(eval("{{if 1}}  a  {{/if}}|"), "a|");
}

#[test]
fn if_conditions_read_variables_and_macros() {
    let vars = [("flag", "1"), ("empty", ""), ("zero", "0"), ("word", "off")];
    assert_eq!(eval_with(&vars, "{{if .flag}}on{{else}}off{{/if}}").0, "on");
    assert_eq!(
        eval_with(&vars, "{{if .empty}}on{{else}}off{{/if}}").0,
        "off"
    );
    assert_eq!(
        eval_with(&vars, "{{if .zero}}on{{else}}off{{/if}}").0,
        "off"
    );
    assert_eq!(
        eval_with(&vars, "{{if .word}}on{{else}}off{{/if}}").0,
        "off"
    );
    assert_eq!(
        eval_with(&vars, "{{if .missing}}on{{else}}off{{/if}}").0,
        "off"
    );
    assert_eq!(
        eval_with(&vars, "{{if !.missing}}on{{else}}off{{/if}}").0,
        "on"
    );
    assert_eq!(eval_with(&vars, "{{if {{getvar::flag}}}}on{{/if}}").0, "on");
    assert_eq!(eval_with(&vars, "{{if {{.flag == 1}}}}eq{{/if}}").0, "eq");
    // A bare macro name is resolved first.
    assert_eq!(eval("{{if char}}{{char}}{{/if}}"), "Шарлотта");
    let mut globals = run_with(&[], &[("theme", "dark")]);
    assert_eq!(
        globals.evaluate("{{if $theme}}{{getglobalvar::theme}}{{/if}}"),
        "dark"
    );
}

#[test]
fn if_nests_and_only_runs_the_chosen_branch() {
    let text = "{{if 1}}A{{if 0}}B{{else}}C{{if 1}}D{{/if}}{{/if}}{{else}}E{{/if}}";
    assert_eq!(eval(text), "ACD");
    let text = "{{if 0}}{{setvar::hit::then}}{{else}}{{setvar::hit::else}}{{/if}}{{getvar::hit}}";
    let (out, run) = eval_with(&[], text);
    assert_eq!(out, "else");
    assert_eq!(run.vars().get("hit").map(String::as_str), Some("else"));
    // The skipped branch rolls no dice.
    let (_, run) = eval_with(&[], "{{if 0}}{{roll::1d20}}{{/if}}");
    assert!(run.rolls().is_empty());
    // An else inside a nested if belongs to it.
    assert_eq!(
        eval("{{if 0}}{{if 1}}x{{else}}y{{/if}}{{else}}z{{/if}}"),
        "z"
    );
}

#[test]
fn if_branches_are_trimmed_and_dedented() {
    let text = "{{if 1}}\n    # Title\n    body\n{{/if}}";
    assert_eq!(eval(text), "# Title\nbody");
}

#[test]
fn scoped_setvar_trim_and_comment() {
    let (out, run) = eval_with(
        &[],
        "{{setvar::long}}\n  line one\n  line two\n{{/setvar}}[{{getvar::long}}]",
    );
    assert_eq!(out, "[line one\nline two]");
    assert_eq!(
        run.vars().get("long").map(String::as_str),
        Some("line one\nline two")
    );
    assert_eq!(
        eval("a{{//}}{{setvar::x::1}}hidden{{///}}b{{getvar::x}}"),
        "ab"
    );
    assert_eq!(eval("<{{trim}}  x  {{/trim}}>"), "<x>");
}

#[test]
fn space_form_arguments() {
    assert_eq!(eval_with(&[("hp", "7")], "{{getvar hp}}").0, "7");
    assert_eq!(eval("{{setvar a::5}}{{getvar::a}}"), "5");
    assert_eq!(eval("{{roll 1d1+2}}"), "3");
    // Unknown names in the space form stay as written.
    assert_eq!(eval("{{some thing}}"), "{{some thing}}");
}

#[test]
fn random_and_pick_split_one_comma_list() {
    let out = eval("{{random::a,b,c}}");
    assert!(["a", "b", "c"].contains(&out.as_str()), "{out}");
    let out = eval("{{random:x::y}}");
    assert!(["x", "y"].contains(&out.as_str()), "{out}");
    assert_eq!(eval("{{random::only\\, one}}"), "only, one");
}

#[test]
fn variable_shorthand_operators() {
    let vars = [("hp", "10"), ("name", "Ann"), ("zero", "0")];
    let check = |text: &str, want: &str| assert_eq!(eval_with(&vars, text).0, want, "{text}");
    check("{{.hp}}", "10");
    check("{{.hp++}}", "11");
    check("{{.hp--}}", "9");
    check("{{.hp = 3}}{{.hp}}", "3");
    check("{{.hp += 5}}{{.hp}}", "15");
    check("{{.hp -= 4}}{{.hp}}", "6");
    check("{{.hp -= abc}}{{.hp}}", "10");
    check("{{.name += !}}{{.name}}", "Ann!");
    check("{{.zero || fallback}}", "fallback");
    check("{{.hp || fallback}}", "10");
    check("{{.zero ?? fallback}}", "0");
    check("{{.none ?? fallback}}", "fallback");
    check("{{.zero ||= 7}}{{.zero}}", "77");
    check("{{.none ??= 8}}{{.none}}", "88");
    check("{{.hp ??= 8}}", "10");
    check("{{.hp == 10}}", "true");
    check("{{.hp != 10}}", "false");
    check(
        "{{.hp > 9}}{{.hp >= 10}}{{.hp < 10}}{{.hp <= 10}}",
        "truetruefalsetrue",
    );
    check("{{.name > 1}}", "false");
    check("{{.hp = {{roll::1d1+1}}}}{{.hp}}", "2");
    check("{{.my-var = x}}{{getvar::my-var}}", "x");
    check("{{.9bad}}", "{{.9bad}}");
    let mut run = run();
    assert_eq!(run.evaluate("{{$g = on}}{{$g}}{{getglobalvar::g}}"), "onon");
    assert_eq!(run.globals().get("g").map(String::as_str), Some("on"));
    assert!(run.vars().is_empty());
}

#[test]
fn shorthand_value_is_lazy() {
    let (_, run) = eval_with(&[("hp", "10")], "{{.hp || {{roll::1d20}}}}");
    assert!(run.rolls().is_empty());
    let (_, run) = eval_with(&[("hp", "0")], "{{.hp || {{roll::1d20}}}}");
    assert_eq!(run.rolls().len(), 1);
}

#[test]
fn varkey_reads_and_writes_json() {
    let vars = [
        ("arr", "[\"a\",{\"k\":1},5,true,null]"),
        ("obj", "{\"x\":\"7\",\"1\":\"one\"}"),
        ("plain", "hello"),
        ("str", "\"abc\""),
    ];
    let check = |text: &str, want: &str| assert_eq!(eval_with(&vars, text).0, want, "{text}");
    check("{{getvarkey::arr::0}}", "a");
    check("{{getvarkey::arr::1}}", "{\"k\":1}");
    check("{{getvarkey::arr::2}}", "5");
    check("{{getvarkey::arr::3}}", "1");
    check("{{getvarkey::arr::4}}", "null");
    check("{{getvarkey::arr::9}}", "");
    check("{{getvarkey::obj::x}}", "7");
    check("{{getvarkey::obj::01}}", "one");
    check("{{getvarindex::plain::0}}", "hello");
    check("{{getvarkey::str::1}}", "b");
    check("{{getvarkey::missing::0}}", "");
    let (_, after) = eval_with(
        &vars,
        "{{setvarkey::new::2::c}}{{setvarkey::o::name::Bo}}{{setvarkey::arr::0::z}}{{setvarkey::plain::0::q}}",
    );
    assert_eq!(
        after.vars().get("new").map(String::as_str),
        Some("[null,null,\"c\"]")
    );
    assert_eq!(
        after.vars().get("o").map(String::as_str),
        Some("{\"name\":\"Bo\"}")
    );
    assert!(after.vars()["arr"].starts_with("[\"z\","));
    assert_eq!(after.vars().get("plain").map(String::as_str), Some("hello"));
    let mut globals = run();
    assert_eq!(
        globals.evaluate("{{setglobalvarkey::g::k::v}}{{getglobalvarkey::g::k}}"),
        "v"
    );
}

#[test]
fn chat_macros_resolve_by_key_and_builtins_win() {
    assert_eq!(eval("В {{town}}, {{TOWN}}."), "В Ривертон, Ривертон.");
    assert_eq!(eval("{{unknownmacro}}"), "{{unknownmacro}}");
    assert_eq!(eval("{{char}}"), "Шарлотта");
}
