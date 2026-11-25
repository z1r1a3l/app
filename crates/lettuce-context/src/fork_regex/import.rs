//! Reading regex scripts out of the files people share: a SillyTavern regex
//! export (one script or an array), an ST/Tavo chat-completion preset
//! (`extensions.regex_scripts`), an ST character card (V2/V3
//! `data.extensions.regex_scripts`) and a Lumiverse regex pack.

use serde_json::Value;

use super::{RegexScript, placement};

/// Scripts found in a file and what kind of file it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexImport {
    pub kind: &'static str,
    pub name: Option<String>,
    pub scripts: Vec<RegexScript>,
}

fn stable_id(seed: &str, index: usize) -> String {
    let hash = blake3::hash(format!("{index}:{seed}").as_bytes());
    let hex = hash.to_hex();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn finish(mut scripts: Vec<RegexScript>) -> Vec<RegexScript> {
    let mut seen = std::collections::HashSet::new();
    for (index, script) in scripts.iter_mut().enumerate() {
        if script.id.trim().is_empty() || !seen.insert(script.id.clone()) {
            script.id = stable_id(&format!("{}{}", script.script_name, script.find_regex), index);
            seen.insert(script.id.clone());
        }
        if script.script_name.trim().is_empty() {
            script.script_name = format!("Regex {}", index + 1);
        }
    }
    scripts
}

fn st_scripts(value: &Value) -> Vec<RegexScript> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item.is_object())
        .filter_map(|item| serde_json::from_value::<RegexScript>(item.clone()).ok())
        .filter(|script| !script.find_regex.is_empty())
        .collect()
}

/// Regex scripts bundled in a SillyTavern preset, if any.
#[must_use]
pub fn scripts_from_preset(preset: &Value) -> Vec<RegexScript> {
    finish(st_scripts(
        preset
            .pointer("/extensions/regex_scripts")
            .unwrap_or(&Value::Null),
    ))
}

/// Regex scripts embedded in a SillyTavern character card, if any.
#[must_use]
pub fn scripts_from_card(card: &Value) -> Vec<RegexScript> {
    let found = card
        .pointer("/data/extensions/regex_scripts")
        .or_else(|| card.pointer("/extensions/regex_scripts"))
        .unwrap_or(&Value::Null);
    finish(st_scripts(found))
}

fn lumi_script(item: &Value) -> Option<RegexScript> {
    let find = item.get("find_regex").and_then(Value::as_str)?;
    if find.is_empty() {
        return None;
    }
    let flags = item.get("flags").and_then(Value::as_str).unwrap_or("g");
    let strings = |key: &str| -> Vec<String> {
        item.get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let targets = strings("target");
    let display = targets.is_empty() || targets.iter().any(|t| t == "display");
    let prompt = targets.iter().any(|t| t == "prompt");
    let placements = strings("placement")
        .iter()
        .filter_map(|p| match p.as_str() {
            "user_input" => Some(placement::USER_INPUT),
            "ai_output" => Some(placement::AI_OUTPUT),
            "slash_command" => Some(placement::SLASH_COMMAND),
            "world_info" => Some(placement::WORLD_INFO),
            "reasoning" => Some(placement::REASONING),
            _ => None,
        })
        .collect::<Vec<_>>();
    let depth = |key: &str| item.get(key).and_then(Value::as_i64);
    Some(RegexScript {
        id: item
            .get("script_id")
            .or_else(|| item.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        script_name: item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        find_regex: format!("/{find}/{flags}"),
        replace_string: item
            .get("replace_string")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        trim_strings: strings("trim_strings"),
        placement: if placements.is_empty() {
            vec![placement::AI_OUTPUT]
        } else {
            placements
        },
        disabled: item.get("disabled").and_then(Value::as_bool).unwrap_or(false),
        markdown_only: display,
        prompt_only: prompt,
        run_on_edit: item.get("run_on_edit").and_then(Value::as_bool).unwrap_or(true),
        substitute_regex: match item.get("substitute_macros").and_then(Value::as_str) {
            Some("raw") => 1,
            Some("escaped") => 2,
            _ => 0,
        },
        min_depth: depth("min_depth"),
        max_depth: depth("max_depth"),
    })
}

/// Reads any supported regex file. `None` when the JSON holds no scripts.
#[must_use]
pub fn parse_regex_import(json: &str) -> Option<RegexImport> {
    let value: Value = serde_json::from_str(json).ok()?;
    let name = value
        .get("name")
        .or_else(|| value.pointer("/data/name"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let found = if value.is_array() {
        Some(("sillytavern_regex", finish(st_scripts(&value))))
    } else if value.get("findRegex").is_some() {
        Some((
            "sillytavern_regex",
            finish(st_scripts(&Value::Array(vec![value.clone()]))),
        ))
    } else if value.pointer("/extensions/regex_scripts").is_some() && value.get("prompts").is_some() {
        Some(("sillytavern_preset", scripts_from_preset(&value)))
    } else if value.pointer("/data/extensions/regex_scripts").is_some() {
        Some(("character_card", scripts_from_card(&value)))
    } else if let Some(scripts) = value.get("scripts").and_then(Value::as_array) {
        Some((
            "lumiverse_regex",
            finish(scripts.iter().filter_map(lumi_script).collect()),
        ))
    } else if let Some(scripts) = value.pointer("/extensions/regex_scripts") {
        Some(("sillytavern_regex", finish(st_scripts(scripts))))
    } else {
        None
    };
    let (kind, scripts) = found?;
    (!scripts.is_empty()).then_some(RegexImport {
        kind,
        name,
        scripts,
    })
}
