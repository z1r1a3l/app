//! Reads the helper's change lines.
//!
//! ```text
//! [Scene] Time: evening -> night
//! [Characters] Aqua · Outfit: blue dress -> bath towel · Location: bath
//! [Characters] - Darkness
//! [Bonds] Aqua · Trust: 40 -> 55
//! [Inventory] + Kazuma: rope, torch
//! [Inventory] - Kazuma: rope
//! [NPC] Innkeeper · Attitude: wary -> friendly
//! [Notebook] + T1: the innkeeper is a demon | hidden from: Aqua, Kazuma
//! [Notebook] Clue T1: his shadow has horns
//! [Notebook] Reveal T1
//! No changes
//! ```
//!
//! A section tag alone on a line applies to the lines under it. Only the
//! value after the last arrow is kept; an empty value removes the field.

use super::model::{Op, Tracker};

/// What a helper answer said.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedDelta {
    pub ops: Vec<Op>,
    /// The answer said "No changes" (and nothing else was understood).
    pub no_changes: bool,
    /// Lines that were not understood.
    pub skipped: Vec<String>,
}

/// Byte index of ASCII `needle` in `haystack` at or after `from`, ignoring
/// ASCII case (indices stay valid for the original text).
fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() {
        return None;
    }
    (from..=bytes.len() - needle.len())
        .find(|&i| bytes[i..i + needle.len()].eq_ignore_ascii_case(needle))
}

fn strip_block(text: &str, open: &str, close: &str) -> String {
    let mut out = String::new();
    let mut rest = 0;
    while let Some(start) = find_ci(text, open, rest) {
        out.push_str(&text[rest..start]);
        match find_ci(text, close, start) {
            Some(end) => rest = end + close.len(),
            None => {
                rest = text.len();
                break;
            }
        }
    }
    out.push_str(&text[rest..]);
    out
}

fn inner_block(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = find_ci(text, &open, 0)? + open.len();
    let end = find_ci(text, &close, start).unwrap_or(text.len());
    Some(text[start..end].to_owned())
}

fn section(tag: &str) -> Option<Tracker> {
    let tag = tag
        .trim()
        .trim_matches(|c| c == '*' || c == '#')
        .trim()
        .to_lowercase();
    Some(match tag.as_str() {
        "scene" | "world" | "world state" | "сцена" | "мир" => Tracker::Scene,
        "characters" | "character" | "chars" | "персонажи" | "персонаж" => {
            Tracker::Characters
        }
        "bonds" | "bond" | "relationships" | "relationship" | "отношения" | "связи" => {
            Tracker::Bonds
        }
        "inventory" | "items" | "инвентарь" | "предметы" => Tracker::Inventory,
        "npc" | "npcs" | "нпс" | "неписи" => Tracker::Npcs,
        "notebook" | "truths" | "блокнот" | "тайны" => Tracker::Notebook,
        _ => return None,
    })
}

fn is_no_changes(line: &str) -> bool {
    let line = line
        .trim()
        .trim_matches(|c: char| c == '.' || c == '*' || c == '(' || c == ')')
        .to_lowercase();
    matches!(
        line.as_str(),
        "no changes"
            | "no change"
            | "nothing changed"
            | "без изменений"
            | "изменений нет"
            | "нет изменений"
    )
}

fn clean_name(name: &str) -> String {
    name.trim()
        .trim_matches(|c| matches!(c, '*' | '_' | '"' | '«' | '»' | '`'))
        .trim()
        .to_owned()
}

/// `Field: value` → (field, value as written).
fn split_field(text: &str) -> Option<(String, String)> {
    let (name, value) = text.split_once(':')?;
    let name = clean_name(name);
    (!name.is_empty()).then(|| (name, value.trim().to_owned()))
}

/// The value after the last arrow, with "none" markers read as removal.
fn new_value(value: &str) -> String {
    let mut cut = 0;
    for arrow in ["->", "→", "=>", "⟶"] {
        if let Some(index) = value.rfind(arrow) {
            cut = cut.max(index + arrow.len());
        }
    }
    let value = value[cut..].trim();
    let bare = value.trim_matches(|c| c == '(' || c == ')').to_lowercase();
    if matches!(
        bare.as_str(),
        "" | "-" | "—" | "–" | "none" | "removed" | "убрано" | "удалено"
    ) {
        String::new()
    } else {
        value.to_owned()
    }
}

fn removal(body: &str) -> Option<&str> {
    for prefix in ["- ", "− ", "– ", "x ", "remove ", "removed ", "gone "] {
        if body.len() > prefix.len() && body.is_char_boundary(prefix.len()) {
            let (head, rest) = body.split_at(prefix.len());
            if head.eq_ignore_ascii_case(prefix) {
                return Some(rest.trim().trim_start_matches(':').trim());
            }
        }
    }
    None
}

const SEPARATORS: [&str; 4] = [" · ", "·", " • ", " | "];

/// `Name · Field: value · Field: value` (also `Name.Field: value`).
fn entity_fields(body: &str) -> Option<(String, Vec<(String, String)>)> {
    let (name, rest) = SEPARATORS
        .iter()
        .find_map(|sep| body.split_once(sep))
        .or_else(|| {
            let dot = body.find('.')?;
            let colon = body.find(':')?;
            (dot < colon).then(|| (&body[..dot], &body[dot + 1..]))
        })?;
    let name = clean_name(name);
    if name.is_empty() {
        return None;
    }
    let mut pieces = vec![rest.to_owned()];
    for sep in SEPARATORS.iter().chain(&[";"]) {
        pieces = pieces
            .iter()
            .flat_map(|piece| piece.split(sep).map(str::to_owned).collect::<Vec<_>>())
            .collect();
    }
    let fields = pieces
        .iter()
        .filter_map(|piece| split_field(piece))
        .map(|(field, value)| (field, new_value(&value)))
        .collect::<Vec<_>>();
    (!fields.is_empty()).then_some((name, fields))
}

fn truth_id(text: &str) -> Option<String> {
    let text = clean_name(text);
    let text = text.trim_start_matches('#');
    let digits = text
        .strip_prefix(['T', 't', 'Т', 'т'])
        .unwrap_or(text)
        .trim();
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| format!("T{digits}"))
}

fn notebook(body: &str) -> Option<Vec<Op>> {
    let lower = body.to_lowercase();
    if let Some(rest) = body.strip_prefix('+') {
        let (id, rest) = split_field(rest)?;
        let id = truth_id(&id)?;
        let mut parts = rest.split('|');
        let truth = parts.next().unwrap_or_default().trim().to_owned();
        let hidden_from = parts
            .filter_map(split_field)
            .find(|(label, _)| {
                let label = label.to_lowercase();
                label.contains("hidden") || label.contains("скрыт")
            })
            .map(|(_, value)| value)
            .unwrap_or_default();
        return (!truth.is_empty()).then(|| {
            vec![Op::Truth {
                id,
                truth,
                hidden_from,
            }]
        });
    }
    for word in ["clue", "улика", "подсказка"] {
        if lower.starts_with(word) {
            let rest = body.get(word.len()..)?.trim();
            let (id, text) = rest
                .split_once(':')
                .or_else(|| rest.split_once(" - "))
                .or_else(|| rest.split_once(' '))?;
            let text = text
                .trim()
                .trim_matches(|c| c == '"' || c == '«' || c == '»')
                .trim();
            if text.is_empty() {
                return None;
            }
            return Some(vec![Op::Clue {
                id: truth_id(id)?,
                text: text.to_owned(),
                found: None,
            }]);
        }
    }
    for word in [
        "revealed",
        "reveal",
        "truth-",
        "раскрыто",
        "раскрыта",
        "раскрыть",
    ] {
        if lower.starts_with(word) {
            let rest = body
                .get(word.len()..)?
                .trim()
                .trim_start_matches(':')
                .trim();
            let id = rest.split_whitespace().next()?;
            return Some(vec![Op::Reveal {
                id: truth_id(id.trim_end_matches([',', '.', ':']))?,
                revealed: true,
            }]);
        }
    }
    None
}

fn inventory(body: &str) -> Option<Vec<Op>> {
    let (add, rest) = if let Some(rest) = body.strip_prefix('+') {
        (true, rest)
    } else {
        let rest = body
            .strip_prefix('-')
            .or_else(|| body.strip_prefix('−'))
            .or_else(|| body.strip_prefix('–'))?;
        (false, rest)
    };
    let (owner, items) = split_field(rest)?;
    let ops = items
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            if add {
                Op::ItemAdd {
                    owner: owner.clone(),
                    item: item.to_owned(),
                }
            } else {
                Op::ItemRemove {
                    owner: owner.clone(),
                    item: item.to_owned(),
                }
            }
        })
        .collect::<Vec<_>>();
    (!ops.is_empty()).then_some(ops)
}

fn line_ops(tracker: Tracker, body: &str) -> Option<Vec<Op>> {
    match tracker {
        Tracker::Scene => {
            let (field, value) = split_field(body)?;
            Some(vec![Op::Set {
                tracker,
                entity: String::new(),
                field,
                value: new_value(&value),
            }])
        }
        Tracker::Characters | Tracker::Bonds | Tracker::Npcs => {
            let mut body = body;
            if let Some(rest) = removal(body) {
                let name = clean_name(rest);
                if name.is_empty() {
                    return None;
                }
                if !name.contains(':') {
                    return Some(vec![Op::Remove {
                        tracker,
                        entity: name,
                    }]);
                }
                body = rest;
            }
            let (entity, fields) = entity_fields(body)?;
            Some(
                fields
                    .into_iter()
                    .map(|(field, value)| Op::Set {
                        tracker,
                        entity: entity.clone(),
                        field,
                        value,
                    })
                    .collect(),
            )
        }
        Tracker::Inventory => inventory(body),
        Tracker::Notebook => notebook(body),
    }
}

/// Reads a helper answer. Thinking blocks and code fences are ignored; a
/// `<state_update>` block, when present, is the only part read.
#[must_use]
pub fn parse_delta(text: &str) -> ParsedDelta {
    let text = strip_block(text, "<think>", "</think>");
    let text = strip_block(&text, "<thinking>", "</thinking>");
    let text = inner_block(&text, "state_update").unwrap_or(text);
    let mut parsed = ParsedDelta::default();
    let mut current: Option<Tracker> = None;
    let mut said_none = false;
    for raw in text.lines() {
        let mut line = raw.trim();
        if line.starts_with("```") || line.is_empty() {
            continue;
        }
        for bullet in ["* ", "• ", "· "] {
            if let Some(rest) = line.strip_prefix(bullet) {
                line = rest.trim();
            }
        }
        if line.starts_with("- [") {
            line = line[2..].trim();
        }
        if is_no_changes(line) {
            said_none = true;
            continue;
        }
        let mut body = line;
        if let Some(rest) = line.strip_prefix('[')
            && let Some((tag, after)) = rest.split_once(']')
            && let Some(tracker) = section(tag)
        {
            current = Some(tracker);
            body = after.trim();
            if body.is_empty() || is_no_changes(body) {
                said_none |= !body.is_empty();
                continue;
            }
        } else if let Some(tag) = line.strip_suffix(':')
            && let Some(tracker) = section(tag)
        {
            current = Some(tracker);
            continue;
        }
        match current.and_then(|tracker| line_ops(tracker, body)) {
            Some(mut ops) => parsed.ops.append(&mut ops),
            None => parsed.skipped.push(line.to_owned()),
        }
    }
    parsed.no_changes = said_none && parsed.ops.is_empty();
    parsed
}
