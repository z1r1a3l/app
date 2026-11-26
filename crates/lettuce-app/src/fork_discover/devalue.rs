//! Decodes SvelteKit `__data.json` responses (the `devalue` format): each
//! node is a flat array where index 0 is the root and other numbers point at
//! entries. Streamed promises arrive as later `chunk` lines.

use std::collections::HashMap;

use serde_json::{Map, Value};

/// Decoded page data: one value per route node (`None` when skipped), and
/// resolved promises by id.
#[derive(Debug, Default)]
pub(crate) struct SvelteData {
    pub nodes: Vec<Option<Value>>,
    pub chunks: HashMap<u64, Value>,
}

impl SvelteData {
    /// The last data node: the page's own `load` output.
    pub fn page(&self) -> Option<&Value> {
        self.nodes.iter().rev().find_map(Option::as_ref)
    }

    /// A promise placeholder resolved from its chunk, or the value itself.
    pub fn resolve<'a>(&'a self, value: &'a Value) -> Option<&'a Value> {
        match value.get("$promise").and_then(Value::as_u64) {
            Some(id) => self.chunks.get(&id),
            None => Some(value),
        }
    }
}

pub(crate) fn parse(body: &[u8]) -> Option<SvelteData> {
    let text = std::str::from_utf8(body).ok()?;
    let mut data = SvelteData::default();
    let mut saw_data = false;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line).ok()?;
        match value.get("type").and_then(Value::as_str) {
            Some("data") => {
                saw_data = true;
                for node in value.get("nodes").and_then(Value::as_array).into_iter().flatten() {
                    let decoded = match node.get("type").and_then(Value::as_str) {
                        Some("data") => node.get("data").and_then(Value::as_array).map(|flat| decode(flat)),
                        _ => None,
                    };
                    data.nodes.push(decoded);
                }
            }
            Some("chunk") => {
                if let (Some(id), Some(flat)) = (value.get("id").and_then(Value::as_u64), value.get("data").and_then(Value::as_array)) {
                    data.chunks.insert(id, decode(flat));
                }
            }
            _ => {}
        }
    }
    saw_data.then_some(data)
}

/// Hydrates a flat devalue array from its root. Cycles are cut (devalue can
/// encode them; page data never needs them).
pub(crate) fn decode(flat: &[Value]) -> Value {
    fn at(flat: &[Value], index: i64, depth: usize) -> Value {
        if depth > 64 {
            return Value::Null;
        }
        match index {
            -1 | -2 => return Value::Null,
            -3 | -4 | -5 | -6 => return Value::Null,
            _ => {}
        }
        let Some(value) = usize::try_from(index).ok().and_then(|i| flat.get(i)) else {
            return Value::Null;
        };
        match value {
            Value::Array(items) => {
                if let Some(Value::String(tag)) = items.first() {
                    return match tag.as_str() {
                        "Date" | "BigInt" | "URL" | "RegExp" => items.get(1).cloned().unwrap_or(Value::Null),
                        "Promise" => {
                            let mut map = Map::new();
                            map.insert("$promise".to_owned(), items.get(1).cloned().unwrap_or(Value::Null));
                            Value::Object(map)
                        }
                        "Set" => Value::Array(items[1..].iter().map(|i| child(flat, i, depth)).collect()),
                        "Map" => {
                            let mut map = Map::new();
                            for pair in items[1..].chunks(2) {
                                if let [key, value] = pair {
                                    let key = child(flat, key, depth);
                                    let key = key.as_str().map_or_else(|| key.to_string(), str::to_owned);
                                    map.insert(key, child(flat, value, depth));
                                }
                            }
                            Value::Object(map)
                        }
                        "null" => {
                            let mut map = Map::new();
                            for pair in items[1..].chunks(2) {
                                if let [Value::String(key), value] = pair {
                                    map.insert(key.clone(), child(flat, value, depth));
                                }
                            }
                            Value::Object(map)
                        }
                        _ => Value::Null,
                    };
                }
                Value::Array(items.iter().map(|i| child(flat, i, depth)).collect())
            }
            Value::Object(fields) => Value::Object(fields.iter().map(|(k, i)| (k.clone(), child(flat, i, depth))).collect()),
            other => other.clone(),
        }
    }
    fn child(flat: &[Value], pointer: &Value, depth: usize) -> Value {
        pointer.as_i64().map_or(Value::Null, |index| at(flat, index, depth + 1))
    }
    at(flat, 0, 0)
}
