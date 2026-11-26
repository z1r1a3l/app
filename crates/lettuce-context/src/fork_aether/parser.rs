//! Parse AetherKit card shorthand and emit `{{setvar}}` writes.
//!
//! The official SDK conversion regex is not public. The stored values follow
//! the assumptions recorded in `/workspace/aetherkit-notes.md`: flags store
//! the bare name, hooks store a condition plus a `➤ <fn:…>` (or `⇥ <page:…>`)
//! call, and specs / state / dice store their trimmed inner body.

use std::fmt;

/// One parsed declaration, in the order it appears in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub var: String,
    pub value: String,
}

/// A declaration the parser understood, with the span to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hit {
    start: usize,
    end: usize,
    declaration: Declaration,
}

/// Replaces every complete AetherKit shorthand in `text` with a silent
/// `{{setvar::name::value}}`. Unrecognised or unclosed `[[…]]` is left as
/// written. Nested `[[track]]` inside a tracks spec is kept in the body.
#[must_use]
pub fn adapt_text(text: &str) -> String {
    let hits = find_hits(text);
    if hits.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for hit in hits {
        out.push_str(&text[cursor..hit.start]);
        out.push_str(&setvar(&hit.declaration.var, &hit.declaration.value));
        cursor = hit.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// The declarations in `text`, left to right. Later writes to the same slot
/// win once macros evaluate.
#[must_use]
pub fn declarations(text: &str) -> Vec<Declaration> {
    find_hits(text)
        .into_iter()
        .map(|hit| hit.declaration)
        .collect()
}

#[must_use]
pub fn setvar(name: &str, value: &str) -> String {
    format!("{{{{setvar::{name}::{value}}}}}")
}

fn find_hits(text: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut index = 0;
    while let Some(rel) = text[index..].find("[[") {
        let start = index + rel;
        match parse_one(text, start) {
            Some(hit) => {
                index = hit.end;
                hits.push(hit);
            }
            None => index = start + 2,
        }
    }
    hits
}

fn parse_one(text: &str, start: usize) -> Option<Hit> {
    let after_open = start + 2;
    let close = text[after_open..].find("]]")?;
    let header = text[after_open..after_open + close].trim();
    let header_end = after_open + close + 2;
    if let Some(rest) = header.strip_prefix('/') {
        // A stray closer: not a declaration.
        let _ = rest;
        return None;
    }
    if let Some(flag) = parse_flag_header(header) {
        return Some(Hit {
            start,
            end: header_end,
            declaration: flag,
        });
    }
    let (kind, closer) = parse_block_header(header)?;
    let (body, end) = find_block_body(text, header_end, &closer)?;
    let declaration = match kind {
        BlockKind::Hook(n) => hook_declaration(n, body)?,
        BlockKind::SpecArc => Declaration {
            var: "pluginArcSpec".into(),
            value: trim_body(body),
        },
        BlockKind::SpecTracks => Declaration {
            var: "pluginTracksSpec".into(),
            value: trim_body(body),
        },
        BlockKind::State => Declaration {
            var: "pluginStateTemplate".into(),
            value: trim_body(body),
        },
        BlockKind::Dice(n) => Declaration {
            var: format!("pluginDice{n}"),
            value: trim_body(body),
        },
    };
    if declaration.value.is_empty() {
        return None;
    }
    Some(Hit {
        start,
        end,
        declaration,
    })
}

fn parse_flag_header(header: &str) -> Option<Declaration> {
    let (name, rest) = split_tag(header)?;
    let n = slot_number(&name, "flag")?;
    let value = rest?.trim();
    if value.is_empty() {
        return None;
    }
    Some(Declaration {
        var: format!("flag{n}"),
        value: collapse_ws(value),
    })
}

#[derive(Debug, Clone, Copy)]
enum BlockKind {
    Hook(u8),
    SpecArc,
    SpecTracks,
    State,
    Dice(u8),
}

fn parse_block_header(header: &str) -> Option<(BlockKind, String)> {
    let (name, rest) = split_tag(header)?;
    if let Some(n) = slot_number(&name, "hook") {
        if rest.is_some() {
            return None;
        }
        return Some((BlockKind::Hook(n), format!("hook{n}")));
    }
    if let Some(n) = slot_number(&name, "dice") {
        if rest.is_some() {
            return None;
        }
        return Some((BlockKind::Dice(n), format!("dice{n}")));
    }
    if name.eq_ignore_ascii_case("state") && rest.is_none() {
        return Some((BlockKind::State, "state".into()));
    }
    if name.eq_ignore_ascii_case("spec") {
        let kind = rest?.trim();
        if kind.eq_ignore_ascii_case("arc") {
            return Some((BlockKind::SpecArc, "spec: arc".into()));
        }
        if kind.eq_ignore_ascii_case("tracks") {
            return Some((BlockKind::SpecTracks, "spec: tracks".into()));
        }
    }
    None
}

/// Split `flag1: name` / `spec: arc` / `hook1` into the tag and the optional
/// remainder after the first colon.
fn split_tag(header: &str) -> Option<(String, Option<&str>)> {
    let header = header.trim();
    if header.is_empty() {
        return None;
    }
    match header.split_once(':') {
        Some((name, rest)) => Some((name.trim().to_owned(), Some(rest))),
        None => Some((header.to_owned(), None)),
    }
}

fn slot_number(name: &str, prefix: &str) -> Option<u8> {
    let rest = name.strip_prefix(prefix).or_else(|| {
        if name.len() >= prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix) {
            Some(&name[prefix.len()..])
        } else {
            None
        }
    })?;
    if rest.len() != 1 {
        return None;
    }
    let n = rest.chars().next()?.to_digit(10)? as u8;
    (1..=5).contains(&n).then_some(n)
}

fn find_block_body<'a>(
    text: &'a str,
    body_start: usize,
    closer_name: &str,
) -> Option<(&'a str, usize)> {
    // Closers are `[[/hook1]]`, `[[/spec: arc]]`, `[[/state]]`, …
    // Search case-insensitively for `[[/` then a name match and `]]`.
    let hay = &text[body_start..];
    let mut search = 0;
    while let Some(rel) = hay[search..].find("[[") {
        let at = search + rel;
        let Some(after) = hay[at + 2..].strip_prefix('/') else {
            search = at + 2;
            continue;
        };
        let Some(close) = after.find("]]") else {
            return None;
        };
        let name = after[..close].trim();
        if names_match(name, closer_name) {
            let end = body_start + at + 2 + 1 + close + 2;
            return Some((&hay[..at], end));
        }
        search = at + 2;
    }
    None
}

fn names_match(found: &str, expected: &str) -> bool {
    collapse_ws(found).eq_ignore_ascii_case(&collapse_ws(expected))
}

fn hook_declaration(n: u8, body: &str) -> Option<Declaration> {
    let fields = hook_fields(body);
    let condition = fields
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("condition"))
        .map(|(_, value)| value.as_str());
    let call = fields
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("call"))
        .map(|(_, value)| value.as_str());
    let page = fields
        .iter()
        .find(|(key, _)| {
            key.eq_ignore_ascii_case("page")
                || key.eq_ignore_ascii_case("goto")
                || key.eq_ignore_ascii_case("hijack")
        })
        .map(|(_, value)| value.as_str());
    let call_line = if let Some(page) = page {
        Some(format!("⇥ <page:{}>", page_name(page)))
    } else {
        call.filter(|name| !name.is_empty())
            .map(|name| format!("➤ <fn:{}>", fn_name(name)))
    };
    if condition.is_none() && call_line.is_none() {
        return None;
    }
    let mut value = String::new();
    if let Some(condition) = condition.filter(|value| !value.is_empty()) {
        value.push_str("condition: ");
        value.push_str(condition);
    }
    if let Some(call_line) = call_line {
        if !value.is_empty() {
            value.push('\n');
        }
        value.push_str(&call_line);
    }
    Some(Declaration {
        var: format!("hookCall{n}"),
        value,
    })
}

fn hook_fields(body: &str) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        fields.push((key.to_owned(), value.trim().to_owned()));
    }
    fields
}

fn fn_name(raw: &str) -> String {
    let raw = raw.trim();
    let trimmed = raw.trim_start_matches('<').trim_end_matches('>').trim();
    trimmed
        .strip_prefix("fn:")
        .unwrap_or(trimmed)
        .trim()
        .to_owned()
}

fn page_name(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim()
        .strip_prefix("page:")
        .unwrap_or(raw.trim())
        .trim()
        .to_owned()
}

fn trim_body(body: &str) -> String {
    // Strip a single leading and trailing newline around the block, but keep
    // internal indentation (beat lists are easier to read that way).
    body.trim_matches(|c: char| c == '\n' || c == '\r')
        .trim_end()
        .to_owned()
        .trim_start_matches(|c: char| c == '\n' || c == '\r')
        .trim_end()
        .to_owned()
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl fmt::Display for Declaration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.var, self.value)
    }
}

#[cfg(test)]
mod header_tests {
    use super::*;

    #[test]
    fn slot_numbers_are_one_to_five() {
        assert_eq!(slot_number("flag1", "flag"), Some(1));
        assert_eq!(slot_number("HOOK5", "hook"), Some(5));
        assert_eq!(slot_number("flag0", "flag"), None);
        assert_eq!(slot_number("flag6", "flag"), None);
        assert_eq!(slot_number("flag10", "flag"), None);
    }
}
