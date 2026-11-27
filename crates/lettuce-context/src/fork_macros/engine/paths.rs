//! Fork: nested access in the variable shorthand, `{{.inv[0]}}`,
//! `{{.stats.hp}}`, `{{.stats["max hp"]}}`, and `{{.stats.hp = 5}}`.
//! The stored variable must hold JSON; reads of a missing path give "".

use serde_json::Value;

use crate::fork_macros::number::{js_format, js_trim};

/// One step into a JSON value.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Step {
    Key(String),
    Index(usize),
}

const MAX_INDEX: usize = 10_000;
const MAX_DEPTH: usize = 16;

/// Parses `.key`, `[3]` and `["key"]` steps from the start of `text`.
/// Returns the steps and the rest, or `None` when `text` has no steps.
pub(super) fn parse_steps(text: &str) -> Option<(Vec<Step>, &str)> {
    let mut steps = Vec::new();
    let mut rest = text;
    loop {
        if let Some(after) = rest.strip_prefix('.') {
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(after.len());
            if end == 0 {
                break;
            }
            steps.push(Step::Key(after[..end].to_owned()));
            rest = &after[end..];
        } else if let Some(after) = rest.strip_prefix('[') {
            let close = after.find(']')?;
            let inner = js_trim(&after[..close]);
            let quoted = ['"', '\'']
                .iter()
                .find_map(|q| inner.strip_prefix(*q).and_then(|s| s.strip_suffix(*q)));
            if let Some(key) = quoted {
                steps.push(Step::Key(key.to_owned()));
            } else {
                let at: usize = inner.parse().ok()?;
                steps.push(Step::Index(at));
            }
            rest = &after[close + 1..];
        } else {
            break;
        }
        if steps.len() > MAX_DEPTH {
            return None;
        }
    }
    (!steps.is_empty()).then_some((steps, rest))
}

/// Reads a path out of a stored JSON variable, formatted like `getvarkey`.
pub(super) fn read(stored: Option<&String>, steps: &[Step]) -> String {
    let Some(mut value) = stored.and_then(|text| serde_json::from_str::<Value>(text).ok()) else {
        return String::new();
    };
    for step in steps {
        let next = match (&value, step) {
            (Value::Array(items), Step::Index(at)) => items.get(*at).cloned(),
            (Value::Object(map), Step::Index(at)) => map.get(&at.to_string()).cloned(),
            (Value::Object(map), Step::Key(key)) => map.get(key).cloned(),
            (Value::Array(items), Step::Key(key)) if key == "length" => {
                Some(Value::from(items.len()))
            }
            _ => None,
        };
        let Some(next) = next else {
            return String::new();
        };
        value = next;
    }
    match value {
        Value::Null => String::new(),
        Value::String(text) => text,
        Value::Bool(flag) => if flag { "1" } else { "0" }.to_owned(),
        Value::Number(number) => number
            .as_f64()
            .map_or_else(|| number.to_string(), js_format),
        other => other.to_string(),
    }
}

/// Writes `value` at a path, creating objects and arrays on the way.
/// Returns the new JSON text, or `None` when the stored text is not JSON
/// or the path cannot be written.
pub(super) fn write(stored: Option<&String>, steps: &[Step], value: &str) -> Option<String> {
    let mut root = match stored {
        None => Value::Null,
        Some(text) if text.is_empty() => Value::Null,
        Some(text) => serde_json::from_str::<Value>(text).ok()?,
    };
    let leaf = serde_json::from_str::<Value>(value)
        .ok()
        .filter(|parsed| !parsed.is_null() && !parsed.is_string())
        .unwrap_or_else(|| Value::String(value.to_owned()));
    let mut slot = &mut root;
    for step in steps {
        if slot.is_null() {
            *slot = match step {
                Step::Index(_) => Value::Array(Vec::new()),
                Step::Key(_) => Value::Object(serde_json::Map::new()),
            };
        }
        slot = match (slot, step) {
            (Value::Array(items), Step::Index(at)) => {
                if *at > MAX_INDEX {
                    return None;
                }
                if items.len() <= *at {
                    items.resize(at + 1, Value::Null);
                }
                &mut items[*at]
            }
            (Value::Object(map), Step::Index(at)) => {
                map.entry(at.to_string()).or_insert(Value::Null)
            }
            (Value::Object(map), Step::Key(key)) => map.entry(key.clone()).or_insert(Value::Null),
            _ => return None,
        };
    }
    *slot = leaf;
    Some(root.to_string())
}
