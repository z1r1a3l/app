use super::*;

const VEXA_REGEX: &str = include_str!("../../tests/fixtures/vexa/vexa-v1.0-regex.json");
const GOLDEN: &str = include_str!("../../tests/fixtures/vexa/golden.json");

/// Mirrors `substituteParams` in tests/fixtures/vexa/gen-golden.mjs.
fn golden_macros(text: &str) -> String {
    let user = regress::Regex::with_flags(r"\{\{user\}\}", "i").expect("user");
    let char_re = regress::Regex::with_flags(r"\{\{char\}\}", "i").expect("char");
    let random = regress::Regex::with_flags(r"\{\{random::([^}]*?)\}\}", "i").expect("random");
    let global = regress::Regex::with_flags(r"\{\{getglobalvar::([^}]*?)\}\}", "i").expect("global");
    let step = |re: &regress::Regex, text: &str, f: &dyn Fn(&regress::Match, &str) -> String| {
        let mut out = String::new();
        let mut last = 0;
        for m in re.find_iter(text) {
            out.push_str(&text[last..m.start()]);
            out.push_str(&f(&m, text));
            last = m.end();
        }
        out.push_str(&text[last..]);
        out
    };
    let text = step(&user, text, &|_, _| "Алекс".to_owned());
    let text = step(&char_re, &text, &|_, _| "Шарлотта".to_owned());
    let text = step(&random, &text, &|m, t| {
        let list = m.group(1).map_or("", |r| &t[r]);
        list.split("::").next().unwrap_or("").to_owned()
    });
    step(&global, &text, &|_, _| String::new())
}

fn fixture(name: &str) -> &'static str {
    match name {
        "reply-full.txt" => include_str!("../../tests/fixtures/vexa/reply-full.txt"),
        "reply-short.txt" => include_str!("../../tests/fixtures/vexa/reply-short.txt"),
        "reply-streaming-partial.txt" => {
            include_str!("../../tests/fixtures/vexa/reply-streaming-partial.txt")
        }
        other => panic!("unknown fixture {other}"),
    }
}

#[test]
fn every_vexa_script_parses_and_compiles() {
    let import = parse_regex_import(VEXA_REGEX).expect("vexa pack");
    assert_eq!(import.kind, "sillytavern_regex");
    assert_eq!(import.scripts.len(), 143);
    for script in &import.scripts {
        assert!(
            compile_find_regex(&script.find_regex).is_some(),
            "{} does not compile",
            script.script_name
        );
    }
    let prompt_only = import.scripts.iter().filter(|s| s.prompt_only).count();
    let display_only = import.scripts.iter().filter(|s| s.markdown_only).count();
    assert_eq!((prompt_only, display_only), (10, 134));
}

#[test]
fn vexa_matches_sillytavern_golden_outputs() {
    let scripts = parse_regex_import(VEXA_REGEX).expect("vexa pack").scripts;
    let cases: serde_json::Value = serde_json::from_str(GOLDEN).expect("golden");
    let mut checked = 0;
    for case in cases.as_array().expect("cases") {
        let file = case["file"].as_str().expect("file");
        let target = case["target"].as_str().expect("target");
        let depth = case["depth"].as_i64().expect("depth");
        let expected = case["output"].as_str().expect("output");
        let (target, placement) = match target {
            "display" => (RegexTarget::Display, placement::AI_OUTPUT),
            "prompt" => (RegexTarget::Prompt, placement::AI_OUTPUT),
            "prompt-user" => (RegexTarget::Prompt, placement::USER_INPUT),
            other => panic!("target {other}"),
        };
        let actual = apply_regex_scripts(
            &scripts,
            fixture(file),
            RegexApplyContext {
                placement,
                target,
                depth: Some(depth),
            },
            &golden_macros,
        );
        if actual != expected {
            let at = actual
                .char_indices()
                .zip(expected.chars())
                .find(|((_, a), e)| a != e)
                .map_or(actual.len().min(expected.len()), |((i, _), _)| i);
            let from = actual.floor_char_boundary(at.saturating_sub(120));
            let to = actual.floor_char_boundary((at + 200).min(actual.len()));
            panic!(
                "{file} {target:?} depth {depth}: differs at {at} (len {} vs {})\nactual: …{}…",
                actual.len(),
                expected.len(),
                &actual[from..to]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 18);
}

#[test]
fn old_internal_states_leave_the_prompt_beyond_depth_three() {
    let scripts = parse_regex_import(VEXA_REGEX).expect("vexa pack").scripts;
    let text = fixture("reply-full.txt");
    let context = |depth| RegexApplyContext {
        placement: placement::AI_OUTPUT,
        target: RegexTarget::Prompt,
        depth: Some(depth),
    };
    let recent = apply_regex_scripts(&scripts, text, context(1), &golden_macros);
    let old = apply_regex_scripts(&scripts, text, context(3), &golden_macros);
    assert!(recent.contains("<internal_states>"));
    assert!(!old.contains("<internal_states>"));
    assert!(old.contains("Шарлотта не поднимает глаз"));
}

fn script(find: &str, replace: &str) -> RegexScript {
    RegexScript {
        find_regex: find.to_owned(),
        replace_string: replace.to_owned(),
        ..RegexScript::default()
    }
}

fn display(scripts: &[RegexScript], text: &str) -> String {
    apply_regex_scripts(
        scripts,
        text,
        RegexApplyContext {
            placement: placement::AI_OUTPUT,
            target: RegexTarget::Display,
            depth: Some(0),
        },
        &|s: &str| s.to_owned(),
    )
}

#[test]
fn global_flag_controls_how_many_matches_change() {
    assert_eq!(display(&[script("/a/", "b")], "aaa"), "baa");
    assert_eq!(display(&[script("/a/g", "b")], "aaa"), "bbb");
    assert_eq!(display(&[script("/A/gi", "b")], "aAa"), "bbb");
    // Without slashes the whole string is the pattern (first match only).
    assert_eq!(display(&[script("a.", "x")], "abab"), "xab");
}

#[test]
fn groups_named_groups_and_match_macro_expand() {
    assert_eq!(
        display(&[script(r"/(\w+)@(\w+)/g", "$2 at $1 ({{match}})")], "me@home"),
        "home at me (me@home)"
    );
    assert_eq!(
        display(&[script(r"/(?<word>\w+)!/", "<$<word>>")], "hi!"),
        "<hi>"
    );
    // A group that did not take part, or does not exist, is empty.
    assert_eq!(display(&[script(r"/(x)?y/", "[$1|$7]")], "y"), "[|]");
}

#[test]
fn trim_strings_are_removed_from_groups() {
    let mut s = script(r"/<t>(.*?)<\/t>/g", "$1");
    s.trim_strings = vec!["*".to_owned()];
    assert_eq!(display(&[s], "<t>*a*</t>"), "a");
}

#[test]
fn variable_length_lookbehind_works() {
    assert_eq!(
        display(&[script(r"/(?<=<k>[^<]*)x/g", "y")], "<k>aaxbx</k>x"),
        "<k>aayby</k>x"
    );
}

#[test]
fn depth_limits_follow_sillytavern() {
    let mut s = script("/a/g", "b");
    s.prompt_only = true;
    s.markdown_only = false;
    s.min_depth = Some(2);
    s.max_depth = Some(3);
    let at = |depth| {
        apply_regex_scripts(
            std::slice::from_ref(&s),
            "a",
            RegexApplyContext {
                placement: placement::AI_OUTPUT,
                target: RegexTarget::Prompt,
                depth: Some(depth),
            },
            &|v: &str| v.to_owned(),
        )
    };
    assert_eq!((at(1), at(2), at(3), at(4)), ("a".into(), "b".into(), "b".into(), "a".into()));
}

#[test]
fn targets_respect_display_and_prompt_flags() {
    let mut display_only = script("/a/", "d");
    display_only.markdown_only = true;
    let mut prompt_only = script("/a/", "p");
    prompt_only.markdown_only = false;
    prompt_only.prompt_only = true;
    let mut neither = script("/a/", "n");
    neither.markdown_only = false;
    assert!(display_only.applies_to(RegexTarget::Display) && !display_only.applies_to(RegexTarget::Prompt));
    assert!(!prompt_only.applies_to(RegexTarget::Display) && prompt_only.applies_to(RegexTarget::Prompt));
    assert!(neither.applies_to(RegexTarget::Display) && neither.applies_to(RegexTarget::Prompt));
}

#[test]
fn invalid_patterns_leave_text_unchanged() {
    assert_eq!(display(&[script("/(unclosed/g", "x")], "(unclosed"), "(unclosed");
    assert_eq!(display(&[script("", "x")], "abc"), "abc");
}

#[test]
fn basic_macros_resolve_and_nest() {
    let values = BasicMacroValues {
        user: "Алекс".into(),
        char: "Шарлотта".into(),
        ..Default::default()
    };
    assert_eq!(substitute_basic_macros("{{user}} и {{char}}", &values), "Алекс и Шарлотта");
    assert_eq!(substitute_basic_macros("a{{// comment}}b{{noop}}c", &values), "abc");
    assert_eq!(substitute_basic_macros("x\n{{trim}}\ny", &values), "xy");
    let rolled = substitute_basic_macros("{{roll::1d6}}", &values);
    let n: u32 = rolled.parse().expect("number");
    assert!((1..=6).contains(&n));
    let picked = substitute_basic_macros("{{random::a::b}}", &values);
    assert!(picked == "a" || picked == "b");
    assert_eq!(substitute_basic_macros("{{unknown::x}}", &values), "{{unknown::x}}");
    assert_eq!(substitute_basic_macros("[{{random::{{user}}}}]", &values), "[Алекс]");
}

#[test]
fn lumiverse_and_card_and_preset_imports_read_scripts() {
    let lumi = r#"{"name":"pack","scripts":[{"name":"x","find_regex":"a","replace_string":"b","flags":"gi","placement":["ai_output"],"target":["prompt"],"min_depth":3,"max_depth":null,"trim_strings":[],"run_on_edit":false,"substitute_macros":"none","disabled":false}]}"#;
    let import = parse_regex_import(lumi).expect("lumi");
    assert_eq!(import.kind, "lumiverse_regex");
    let s = &import.scripts[0];
    assert_eq!(s.find_regex, "/a/gi");
    assert!(s.prompt_only && !s.markdown_only);
    assert_eq!(s.min_depth, Some(3));
    assert!(!s.id.is_empty());

    let card = r#"{"spec":"chara_card_v3","data":{"name":"C","extensions":{"regex_scripts":[{"scriptName":"r","findRegex":"/x/g","replaceString":"y","placement":[2]}]}}}"#;
    let import = parse_regex_import(card).expect("card");
    assert_eq!(import.kind, "character_card");
    assert_eq!(import.scripts.len(), 1);

    let preset = r#"{"prompts":[],"extensions":{"regex_scripts":[{"scriptName":"r","findRegex":"/x/g","replaceString":"y","placement":[2],"markdownOnly":true}]}}"#;
    assert_eq!(parse_regex_import(preset).expect("preset").kind, "sillytavern_preset");
    assert!(parse_regex_import(r#"{"prompts":[]}"#).is_none());
}

#[test]
fn scripts_round_trip_in_sillytavern_shape() {
    let scripts = parse_regex_import(VEXA_REGEX).expect("vexa pack").scripts;
    let json = serde_json::to_string(&scripts).expect("json");
    assert!(json.contains("\"scriptName\"") && json.contains("\"findRegex\""));
    let back: Vec<RegexScript> = serde_json::from_str(&json).expect("back");
    assert_eq!(back, scripts);
}
