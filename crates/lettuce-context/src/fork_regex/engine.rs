//! The SillyTavern regex engine, ported from
//! `public/scripts/extensions/regex/engine.js` (`getRegexedString`,
//! `runRegexScript`, `filterString`) and `utils.js` (`regexFromString`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::RegexScript;

/// Which copy of a message a script pass produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegexTarget {
    /// What the chat shows (ST `isMarkdown`).
    Display,
    /// What the model receives (ST `isPrompt`).
    Prompt,
}

/// The facts one pass is run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegexApplyContext {
    /// ST placement number (1 user input, 2 AI output, ...).
    pub placement: u8,
    pub target: RegexTarget,
    /// Messages between this one and the newest (0 = newest), when known.
    pub depth: Option<i64>,
}

/// A compiled find pattern: the regex and whether it replaces every match.
#[derive(Debug)]
pub struct CompiledFind {
    regex: regress::Regex,
    global: bool,
}

const CACHE_LIMIT: usize = 1000;

fn cache() -> &'static Mutex<HashMap<String, Option<Arc<CompiledFind>>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<CompiledFind>>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// ST `regexFromString`: `/pattern/flags`, else the whole input as a pattern.
/// Returns `None` for a pattern that does not compile.
#[must_use]
pub fn compile_find_regex(input: &str) -> Option<Arc<CompiledFind>> {
    if let Some(found) = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(input)
    {
        return found.clone();
    }
    let compiled = compile_uncached(input).map(Arc::new);
    let mut map = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if map.len() >= CACHE_LIMIT {
        map.clear();
    }
    map.insert(input.to_owned(), compiled.clone());
    compiled
}

fn valid_flags(flags: &str) -> bool {
    // ST: /^(?!.*?(.).*?\1)[gmixXsuUAJ]+$/ then `new RegExp` rejects the
    // letters JavaScript does not know (x X U A J).
    let mut seen = std::collections::HashSet::new();
    !flags.is_empty()
        && flags
            .chars()
            .all(|c| "gmixXsuUAJ".contains(c) && seen.insert(c))
}

fn compile_uncached(input: &str) -> Option<CompiledFind> {
    if input.is_empty() {
        return None;
    }
    // ST matches /(\/?)(.+)\1([a-z]*)/i: with a leading slash the pattern
    // runs to the last slash that is followed only by letters.
    let (pattern, flags) = match input.strip_prefix('/') {
        Some(rest) => match rest.rfind('/') {
            Some(end) if end > 0 && rest[end + 1..].chars().all(|c| c.is_ascii_alphabetic()) => {
                (&rest[..end], &rest[end + 1..])
            }
            _ => (input, ""),
        },
        None => (input, ""),
    };
    let (pattern, flags) = if flags.is_empty() || valid_flags(flags) {
        (pattern, flags)
    } else {
        (input, "")
    };
    if flags.chars().any(|c| "xXUAJ".contains(c)) {
        return None;
    }
    let global = flags.contains('g');
    let regex_flags: String = flags.chars().filter(|c| "imsu".contains(*c)).collect();
    regress::Regex::with_flags(pattern, regex_flags.as_str())
        .ok()
        .map(|regex| CompiledFind { regex, global })
}

/// ST `getRegexedString`: runs every applicable script in order.
pub fn apply_regex_scripts<'a, I, M>(
    scripts: I,
    text: &str,
    context: RegexApplyContext,
    macros: &M,
) -> String
where
    I: IntoIterator<Item = &'a RegexScript>,
    M: Fn(&str) -> String + ?Sized,
{
    let mut out = text.to_owned();
    if out.is_empty() {
        return out;
    }
    for script in scripts {
        if script.disabled || !script.applies_to(context.target) {
            continue;
        }
        if let Some(depth) = context.depth {
            if script.min_depth.is_some_and(|min| min >= -1 && depth < min) {
                continue;
            }
            if script.max_depth.is_some_and(|max| max >= 0 && depth > max) {
                continue;
            }
        }
        if script.placement.contains(&context.placement) {
            out = run_regex_script(script, &out, macros);
        }
    }
    out
}

/// ST `runRegexScript`.
pub fn run_regex_script<M>(script: &RegexScript, text: &str, macros: &M) -> String
where
    M: Fn(&str) -> String + ?Sized,
{
    if script.disabled || script.find_regex.is_empty() || text.is_empty() {
        return text.to_owned();
    }
    let find = match script.substitute_regex {
        1 => macros(&script.find_regex),
        2 => escape_macro_values(&script.find_regex, macros),
        _ => script.find_regex.clone(),
    };
    let Some(compiled) = compile_find_regex(&find) else {
        return text.to_owned();
    };
    // A regex engine bug must not take the chat down: such a script is
    // skipped and the text stays as it was.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        replace_all(script, &compiled, text, macros)
    }))
    .unwrap_or_else(|_| text.to_owned())
}

fn replace_all<M>(script: &RegexScript, compiled: &CompiledFind, text: &str, macros: &M) -> String
where
    M: Fn(&str) -> String + ?Sized,
{
    let replace_template = replace_match_macro(&script.replace_string);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for found in compiled.regex.find_iter(text) {
        out.push_str(&text[last..found.start()]);
        let with_groups = expand_groups(&replace_template, |token| {
            let range = match token {
                GroupRef::Index(index) => found.group(index),
                GroupRef::Name(name) => found.named_group(name),
            };
            match range {
                Some(range) if range.start < range.end => {
                    let mut value = text[range].to_owned();
                    for trim in &script.trim_strings {
                        let trim = macros(trim);
                        if !trim.is_empty() {
                            value = value.replace(&trim, "");
                        }
                    }
                    value
                }
                _ => String::new(),
            }
        });
        out.push_str(&macros(&with_groups));
        last = found.end();
        if !compiled.global {
            break;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// `{{match}}` (any case) becomes `$0`.
fn replace_match_macro(template: &str) -> String {
    let lower = template.to_ascii_lowercase();
    if !lower.contains("{{match}}") {
        return template.to_owned();
    }
    let mut out = String::with_capacity(template.len());
    let mut index = 0;
    while let Some(pos) = lower[index..].find("{{match}}") {
        out.push_str(&template[index..index + pos]);
        out.push_str("$0");
        index += pos + "{{match}}".len();
    }
    out.push_str(&template[index..]);
    out
}

enum GroupRef<'a> {
    Index(usize),
    Name(&'a str),
}

/// Replaces `$<digits>` and `$<name>` tokens (ST: /\$(\d+)|\$<([^>]+)>/g).
fn expand_groups<F>(template: &str, mut value: F) -> String
where
    F: FnMut(GroupRef<'_>) -> String,
{
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len());
    let mut index = 0;
    let mut copied = 0;
    while index < bytes.len() {
        if bytes[index] == b'$' && index + 1 < bytes.len() {
            let next = bytes[index + 1];
            if next.is_ascii_digit() {
                let start = index + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                out.push_str(&template[copied..index]);
                let number = template[start..end].parse::<usize>().unwrap_or(usize::MAX);
                out.push_str(&value(GroupRef::Index(number)));
                index = end;
                copied = end;
                continue;
            }
            if next == b'<'
                && let Some(close) = template[index + 2..].find('>')
            {
                let name = &template[index + 2..index + 2 + close];
                if !name.is_empty() {
                    out.push_str(&template[copied..index]);
                    out.push_str(&value(GroupRef::Name(name)));
                    index = index + 2 + close + 1;
                    copied = index;
                    continue;
                }
            }
        }
        index += 1;
    }
    out.push_str(&template[copied..]);
    out
}

/// ST `substitute_find_regex.ESCAPED`: macro values are regex-escaped.
fn escape_macro_values<M>(find: &str, macros: &M) -> String
where
    M: Fn(&str) -> String + ?Sized,
{
    let mut out = String::with_capacity(find.len());
    let mut rest = find;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        match rest[start..].find("}}") {
            Some(end) => {
                let token = &rest[start..start + end + 2];
                let value = macros(token);
                if value == token {
                    out.push_str(token);
                } else {
                    out.push_str(&escape_regex(&value));
                }
                rest = &rest[start + end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn escape_regex(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            '\u{c}' => out.push_str("\\f"),
            '\0' => out.push_str("\\0"),
            '.' | '^' | '$' | '*' | '+' | '?' | '{' | '}' | '[' | ']' | '\\' | '/' | '|' | '('
            | ')' => {
                out.push('\\');
                out.push(c);
            }
            other => out.push(other),
        }
    }
    out
}
