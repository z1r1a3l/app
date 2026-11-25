//! A small SillyTavern macro subset for regex replacements and trim strings:
//! `{{user}}`, `{{char}}`, `{{random::a::b}}` / `{{random:a,b}}`,
//! `{{roll::1d20}}` / `{{roll:1d20}}`, `{{noop}}`, `{{newline}}`, `{{trim}}`,
//! `{{// comment}}`, `{{getvar::x}}` and `{{getglobalvar::x}}`.
//! Unknown macros are left as written. Macros nest; the innermost resolves
//! first.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Values the macros read.
#[derive(Debug, Clone, Default)]
pub struct BasicMacroValues {
    pub user: String,
    pub char: String,
    pub vars: HashMap<String, String>,
    pub global_vars: HashMap<String, String>,
}

static SEED: AtomicU64 = AtomicU64::new(0);

fn next_random(bound: u64) -> u64 {
    if bound <= 1 {
        return 0;
    }
    let mut x = SEED.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64)
            | 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    SEED.store(x, Ordering::Relaxed);
    x % bound
}

fn args(body: &str, name: &str) -> Option<Vec<String>> {
    let lower = body.to_ascii_lowercase();
    if lower == name {
        return Some(Vec::new());
    }
    if let Some(rest) = lower.strip_prefix(name) {
        let raw = &body[body.len() - rest.len()..];
        if let Some(list) = raw.strip_prefix("::") {
            return Some(list.split("::").map(str::to_owned).collect());
        }
        if let Some(list) = raw.strip_prefix(':') {
            return Some(list.split(',').map(|s| s.trim().to_owned()).collect());
        }
        if let Some(list) = raw.strip_prefix(' ') {
            return Some(vec![list.trim().to_owned()]);
        }
    }
    None
}

fn roll(formula: &str) -> Option<String> {
    let formula = formula.trim().to_ascii_lowercase().replace(' ', "");
    let formula = if formula.chars().all(|c| c.is_ascii_digit()) && !formula.is_empty() {
        format!("1d{formula}")
    } else {
        formula
    };
    let (count, rest) = formula.split_once('d')?;
    let count: u64 = if count.is_empty() { 1 } else { count.parse().ok()? };
    let (sides, modifier) = match rest.find(['+', '-']) {
        Some(pos) => (&rest[..pos], rest[pos..].parse::<i64>().ok()?),
        None => (rest, 0),
    };
    let sides: u64 = sides.parse().ok()?;
    if count == 0 || count > 1000 || sides == 0 {
        return None;
    }
    let total: i64 = (0..count).map(|_| next_random(sides) as i64 + 1).sum::<i64>() + modifier;
    Some(total.to_string())
}

fn resolve(body: &str, values: &BasicMacroValues) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.starts_with("//") {
        return Some(String::new());
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "user" => return Some(values.user.clone()),
        "char" => return Some(values.char.clone()),
        "noop" => return Some(String::new()),
        "newline" => return Some("\n".to_owned()),
        _ => {}
    }
    if let Some(list) = args(trimmed, "random") {
        if list.is_empty() {
            return None;
        }
        let pick = next_random(list.len() as u64) as usize;
        return Some(list[pick].clone());
    }
    if let Some(list) = args(trimmed, "roll") {
        return list.first().and_then(|formula| roll(formula));
    }
    if let Some(list) = args(trimmed, "getglobalvar") {
        let name = list.first()?.trim();
        return Some(values.global_vars.get(name).cloned().unwrap_or_default());
    }
    if let Some(list) = args(trimmed, "getvar") {
        let name = list.first()?.trim();
        return Some(values.vars.get(name).cloned().unwrap_or_default());
    }
    None
}

/// Resolves the supported macros in `text`.
#[must_use]
pub fn substitute_basic_macros(text: &str, values: &BasicMacroValues) -> String {
    if !text.contains("{{") {
        return text.to_owned();
    }
    let mut current = text.to_owned();
    // Innermost first: a `{{` whose body holds no further `{{`.
    for _ in 0..64 {
        let mut changed = false;
        let mut out = String::with_capacity(current.len());
        let mut rest = current.as_str();
        while let Some(open) = rest.find("{{") {
            let after = &rest[open + 2..];
            let Some(close) = after.find("}}") else {
                break;
            };
            let body = &after[..close];
            if let Some(inner) = body.rfind("{{") {
                out.push_str(&rest[..open + 2 + inner]);
                rest = &rest[open + 2 + inner..];
                continue;
            }
            out.push_str(&rest[..open]);
            if body.trim().eq_ignore_ascii_case("trim") {
                while out.ends_with('\n') || out.ends_with('\r') {
                    out.pop();
                }
                let mut tail = &after[close + 2..];
                tail = tail.trim_start_matches(['\r', '\n']);
                rest = tail;
                changed = true;
                continue;
            }
            match resolve(body, values) {
                Some(value) => {
                    out.push_str(&value);
                    changed = true;
                }
                None => out.push_str(&rest[open..open + 2 + close + 2]),
            }
            rest = &after[close + 2..];
        }
        out.push_str(rest);
        current = out;
        if !changed {
            break;
        }
    }
    current
}
