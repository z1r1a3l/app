//! Scoped blocks (`{{if c}}…{{else}}…{{/if}}`, `{{setvar::x}}…{{/setvar}}`),
//! the `{{name arg}}` space form, the `{{.var}}` / `{{$var}}` shorthand and
//! `getvarkey` / `setvarkey`, following SillyTavern's macro engine
//! (MacroCstWalker, core-macros `if`, variables.js).

use super::{ArgPiece, MacroRun, Node, normalized, split_pieces};
use crate::fork_macros::number::{js_format, js_number, js_trim};

/// The largest array index `setvarkey` fills (holes become `null`).
const MAX_ARRAY_INDEX: f64 = 10_000.0;

/// How the arguments were written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Form {
    /// `{{name}}`
    Bare,
    /// `{{name::a::b}}`
    Colons,
    /// `{{name a::b}}`
    Space,
}

/// A macro's head: flags, name and argument pieces.
pub(super) struct Header<'a> {
    pub closing: bool,
    pub preserve: bool,
    /// Lowercase.
    pub name: String,
    pub form: Form,
    pub args: Vec<Vec<ArgPiece<'a>>>,
}

pub(super) enum Head<'a> {
    Named(Header<'a>),
    /// `{{.name …}}` / `{{$name …}}`: global, the text after the prefix,
    /// and the pieces after the first one.
    Shorthand(bool, &'a str, &'a [Node]),
    /// `{{//}}` opens a scoped comment, `{{///}}` closes it.
    CommentOpen,
    CommentClose,
    /// Anything else (legacy `{{random:a,b}}`, plain text in braces).
    Other,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic()
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

pub(super) fn pieces_of(nodes: &[Node]) -> Vec<ArgPiece<'_>> {
    nodes
        .iter()
        .map(|node| match node {
            Node::Text(text) => ArgPiece::Text(text),
            Node::Macro(inner) => ArgPiece::Macro(inner),
        })
        .collect()
}

pub(super) fn head_of(inner: &[Node]) -> Head<'_> {
    let Some(Node::Text(first)) = inner.first() else {
        return Head::Other;
    };
    let trimmed = js_trim(first);
    if inner.len() == 1 && trimmed == "//" {
        return Head::CommentOpen;
    }
    if inner.len() == 1 && trimmed == "///" {
        return Head::CommentClose;
    }
    if trimmed.starts_with("//") {
        // `{{// note}}`: an inline comment.
        return Head::Other;
    }
    let mut rest = first.trim_start_matches(char::is_whitespace);
    if let Some(after) = rest.strip_prefix('.') {
        return Head::Shorthand(false, after, &inner[1..]);
    }
    if let Some(after) = rest.strip_prefix('$') {
        return Head::Shorthand(true, after, &inner[1..]);
    }
    let (mut closing, mut preserve) = (false, false);
    loop {
        rest = rest.trim_start_matches(char::is_whitespace);
        match rest.chars().next() {
            Some('/') => closing = true,
            Some('#') => preserve = true,
            Some('!' | '?' | '~' | '>') => {}
            _ => break,
        }
        rest = &rest[1..];
    }
    let Some(start) = rest.chars().next() else {
        return Head::Other;
    };
    if !is_ident_start(start) {
        return Head::Other;
    }
    let end = rest.find(|c: char| !is_ident(c)).unwrap_or(rest.len());
    let name = rest[..end].to_lowercase();
    let after = &rest[end..];
    let more = &inner[1..];
    let spaced = after.trim_start_matches(char::is_whitespace);
    let (form, tail) = if let Some(tail) = spaced.strip_prefix("::") {
        (Form::Colons, tail)
    } else if spaced.starts_with(':') {
        return Head::Other;
    } else if spaced.is_empty() && more.is_empty() {
        (Form::Bare, "")
    } else if after.len() != spaced.len() {
        (Form::Space, spaced)
    } else {
        return Head::Other;
    };
    let mut pieces = vec![ArgPiece::Text(tail)];
    pieces.extend(pieces_of(more));
    let args = if form == Form::Bare {
        Vec::new()
    } else {
        split_pieces(&pieces)
    };
    Head::Named(Header {
        closing,
        preserve,
        name,
        form,
        args,
    })
}

/// Whether `{{name …}}` with `argc` arguments takes a `{{/name}}` body.
pub(super) fn takes_scope(name: &str, argc: usize) -> bool {
    match name {
        "if" | "setvar" | "setglobalvar" | "addvar" | "addglobalvar" => argc == 1,
        "trim" | "comment" | "reverse" => argc == 0,
        _ => false,
    }
}

fn opener_name<'a>(piece: &ArgPiece<'a>) -> Option<(String, bool)> {
    let ArgPiece::Macro(inner) = piece else {
        return None;
    };
    match head_of(inner) {
        Head::Named(header) if header.closing => Some((header.name, true)),
        Head::Named(header) if takes_scope(&header.name, header.args.len()) => {
            Some((header.name, false))
        }
        Head::CommentOpen => Some(("//".to_owned(), false)),
        Head::CommentClose => Some(("//".to_owned(), true)),
        _ => None,
    }
}

/// The index of the `{{/name}}` that closes the opener at `at`.
pub(super) fn matching_close(items: &[ArgPiece<'_>], at: usize, name: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (index, piece) in items.iter().enumerate().skip(at + 1) {
        if let Some((other, closing)) = opener_name(piece)
            && other == name
        {
            if closing {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            } else {
                depth += 1;
            }
        }
    }
    None
}

/// Splits an `{{if}}` body at its own `{{else}}`.
fn split_else<'b, 'a>(
    body: &'b [ArgPiece<'a>],
) -> (&'b [ArgPiece<'a>], Option<&'b [ArgPiece<'a>]>) {
    let mut depth = 0usize;
    for (index, piece) in body.iter().enumerate() {
        let ArgPiece::Macro(inner) = piece else {
            continue;
        };
        if let Head::Named(header) = head_of(inner) {
            if header.name == "if" && header.closing {
                depth = depth.saturating_sub(1);
            } else if header.name == "if" && header.args.len() == 1 {
                depth += 1;
            } else if header.name == "else" && !header.closing && depth == 0 {
                return (&body[..index], Some(&body[index + 1..]));
            }
        }
    }
    (body, None)
}

/// `MacroEngine.trimScopedContent`: trims and removes the first line's
/// indentation from every line.
pub(super) fn trim_scoped(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }
    let indent_of = |line: &str| line.len() - line.trim_start_matches([' ', '\t']).len();
    let base = content
        .split('\n')
        .find(|line| !js_trim(line).is_empty())
        .map_or(0, indent_of);
    if base == 0 {
        return js_trim(content).to_owned();
    }
    let lines: Vec<&str> = content
        .split('\n')
        .map(|line| {
            if indent_of(line) >= base {
                &line[base..]
            } else {
                line.trim_start_matches(char::is_whitespace)
            }
        })
        .collect();
    js_trim(&lines.join("\n")).to_owned()
}

fn is_false_boolean(value: &str) -> bool {
    matches!(
        js_trim(value).to_lowercase().as_str(),
        "off" | "false" | "0"
    )
}

/// `MACRO_VARIABLE_SHORTHAND_PATTERN`: a letter, then word characters or
/// hyphens, not ending in a hyphen. Returns the name's byte length.
fn shorthand_name(text: &str) -> Option<usize> {
    if !text.chars().next().is_some_and(is_ident_start) {
        return None;
    }
    let mut end = text.find(|c: char| !is_ident(c)).unwrap_or(text.len());
    while text[..end].ends_with('-') {
        end -= 1;
    }
    Some(end)
}

const OPERATORS: [&str; 15] = [
    "++", "--", "??=", "??", "||=", "||", "-=", "==", "!=", ">=", ">", "<=", "<", "+=", "=",
];

impl MacroRun {
    /// Evaluates a list of pieces, pairing scoped openers with their closers.
    pub(super) fn eval_items(&mut self, items: &[ArgPiece<'_>], out: &mut String) {
        let mut index = 0;
        while index < items.len() {
            match &items[index] {
                ArgPiece::Text(text) => out.push_str(text),
                ArgPiece::Macro(inner) => {
                    if let Some((name, false)) = opener_name(&items[index])
                        && let Some(close) = matching_close(items, index, &name)
                    {
                        match self.eval_scoped(inner, &items[index + 1..close]) {
                            Some(value) => out.push_str(&value),
                            // Left for the host: the whole block as written.
                            None => out.push_str(&raw_items(&items[index..=close])),
                        }
                        index = close + 1;
                        continue;
                    }
                    if let Some((_, true)) = opener_name(&items[index]) {
                        // A closer without its opener stays as written.
                        out.push_str(&raw_text(inner));
                    } else {
                        let value = self.eval_macro(inner);
                        out.push_str(&value);
                    }
                }
            }
            index += 1;
        }
    }

    fn eval_scoped(&mut self, inner: &[Node], body: &[ArgPiece<'_>]) -> Option<String> {
        self.evaluated += 1;
        let Head::Named(header) = head_of(inner) else {
            // `{{//}}…{{///}}`: dropped unevaluated.
            return Some(String::new());
        };
        if header.name == "comment" {
            return Some(String::new());
        }
        if header.name == "if" {
            return self.eval_if(&header, Some(body));
        }
        let mut content = String::new();
        self.eval_items(body, &mut content);
        if !header.preserve {
            content = trim_scoped(&content);
        }
        let mut args = self.eval_args(&header.args);
        args.push(content);
        Some(
            self.resolve(&header.name, &args)
                .unwrap_or_else(|| format!("{{{{{}}}}}", raw_text_inner(inner))),
        )
    }

    pub(super) fn eval_args(&mut self, args: &[Vec<ArgPiece<'_>>]) -> Vec<String> {
        args.iter()
            .map(|pieces| {
                let mut value = String::new();
                self.eval_items(pieces, &mut value);
                js_trim(&value).to_owned()
            })
            .collect()
    }

    /// `{{if cond}}then{{else}}other{{/if}}` and `{{if cond::then}}`. Only
    /// the chosen branch is evaluated.
    /// `None`: the condition names something only the host knows, so the
    /// block stays as written.
    pub(super) fn eval_if(
        &mut self,
        header: &Header<'_>,
        scoped: Option<&[ArgPiece<'_>]>,
    ) -> Option<String> {
        let (condition, body): (&[ArgPiece<'_>], &[ArgPiece<'_>]) =
            match (scoped, header.args.as_slice()) {
                (Some(body), [condition]) => (condition, body),
                (None, [condition, body]) => (condition, body),
                _ => return Some(String::new()),
            };
        // `!` in front inverts.
        let mut condition: Vec<ArgPiece<'_>> = condition.to_vec();
        let mut inverted = false;
        if let Some(ArgPiece::Text(first)) = condition.first() {
            let start = first.trim_start_matches(char::is_whitespace);
            if let Some(after) = start.strip_prefix('!') {
                inverted = true;
                condition[0] = ArgPiece::Text(after.trim_start_matches(char::is_whitespace));
            }
        }
        if let [ArgPiece::Text(word)] = condition.as_slice() {
            let word = js_trim(word);
            let bare = !word.is_empty() && shorthand_name(word) == Some(word.len());
            if bare && !is_zero_arg_macro(word) && (header.preserve || is_host_name(word)) {
                return None;
            }
        }
        let mut value = String::new();
        self.eval_items(&condition, &mut value);
        let value = js_trim(&value).to_owned();
        let value = if let Some(rest) = value.strip_prefix(['.', '$'])
            && shorthand_name(rest) == Some(rest.len())
        {
            let global = value.starts_with('$');
            normalized(if global {
                self.globals.get(rest)
            } else {
                self.vars.get(rest)
            })
        } else if is_zero_arg_macro(&value) {
            self.resolve(&value.to_lowercase(), &[]).unwrap_or(value)
        } else {
            value
        };
        let mut falsy = value.is_empty() || is_false_boolean(&value);
        if inverted {
            falsy = !falsy;
        }
        let (then_branch, else_branch) = split_else(body);
        let Some(chosen) = (if falsy {
            else_branch
        } else {
            Some(then_branch)
        }) else {
            return Some(String::new());
        };
        let mut out = String::new();
        self.eval_items(chosen, &mut out);
        if header.preserve {
            Some(out)
        } else {
            Some(trim_scoped(&out))
        }
    }

    /// `{{.name}}`, `{{.name = v}}`, `{{$name += v}}`, `{{.name == v}}` …
    pub(super) fn eval_shorthand(
        &mut self,
        global: bool,
        after: &str,
        more: &[Node],
        inner: &[Node],
    ) -> String {
        let raw = || format!("{{{{{}}}}}", raw_text_inner(inner));
        let after = after.trim_start_matches(char::is_whitespace);
        let Some(end) = shorthand_name(after) else {
            return raw();
        };
        let name = after[..end].to_owned();
        if let Some((steps, tail)) = super::paths::parse_steps(&after[end..]) {
            return self
                .eval_path(global, &name, &steps, tail, more)
                .unwrap_or_else(raw);
        }
        let rest = after[end..].trim_start_matches(char::is_whitespace);
        let Some(operator) = OPERATORS.iter().find(|op| rest.starts_with(**op)) else {
            if rest.is_empty() && more.is_empty() {
                return normalized(self.var(global, &name));
            }
            return raw();
        };
        let value_text = &rest[operator.len()..];
        if matches!(*operator, "++" | "--") {
            if !js_trim(value_text).is_empty() || !more.is_empty() {
                return raw();
            }
            return self.add(global, &name, if *operator == "++" { "1" } else { "-1" });
        }
        let mut pieces = vec![ArgPiece::Text(value_text)];
        pieces.extend(pieces_of(more));
        let mut cached: Option<String> = None;
        let mut value = |run: &mut Self| -> String {
            cached
                .get_or_insert_with(|| {
                    let mut out = String::new();
                    run.eval_items(&pieces, &mut out);
                    js_trim(&out).to_owned()
                })
                .clone()
        };
        let exists = |run: &Self| {
            if global {
                run.globals.contains_key(&name)
            } else {
                run.vars.contains_key(&name)
            }
        };
        let current = |run: &Self| normalized(run.var(global, &name));
        let falsy = |text: &str| text.is_empty() || text == "0" || is_false_boolean(text);
        let compare = |run: &mut Self,
                       value: &mut dyn FnMut(&mut Self) -> String,
                       test: fn(f64, f64) -> bool| {
            let left = js_number(&current(run));
            let right = js_number(&value(run));
            match (left, right) {
                (Some(left), Some(right)) if test(left, right) => "true".to_owned(),
                _ => "false".to_owned(),
            }
        };
        match *operator {
            "=" => {
                let value = value(self);
                self.set(global, &name, value);
                String::new()
            }
            "+=" => {
                let value = value(self);
                self.add(global, &name, &value);
                String::new()
            }
            "-=" => {
                if let Some(number) = js_number(&value(self)) {
                    self.add(global, &name, &js_format(-number));
                }
                String::new()
            }
            "||" => {
                let now = current(self);
                if falsy(&now) { value(self) } else { now }
            }
            "??" => {
                if exists(self) {
                    current(self)
                } else {
                    value(self)
                }
            }
            "||=" => {
                let now = current(self);
                if falsy(&now) {
                    let value = value(self);
                    self.set(global, &name, value.clone());
                    value
                } else {
                    now
                }
            }
            "??=" => {
                if exists(self) {
                    current(self)
                } else {
                    let value = value(self);
                    self.set(global, &name, value.clone());
                    value
                }
            }
            "==" => (current(self) == value(self)).to_string(),
            "!=" => (current(self) != value(self)).to_string(),
            ">" => compare(self, &mut value, |a, b| a > b),
            ">=" => compare(self, &mut value, |a, b| a >= b),
            "<" => compare(self, &mut value, |a, b| a < b),
            "<=" => compare(self, &mut value, |a, b| a <= b),
            _ => raw(),
        }
    }

    /// Fork: `{{.stats.hp}}`, `{{.inv[0] = x}}` and comparisons on a path.
    fn eval_path(
        &mut self,
        global: bool,
        name: &str,
        steps: &[super::paths::Step],
        tail: &str,
        more: &[Node],
    ) -> Option<String> {
        let tail = tail.trim_start_matches(char::is_whitespace);
        let current = super::paths::read(self.var(global, name), steps);
        if tail.is_empty() && more.is_empty() {
            return Some(current);
        }
        let operator = ["==", "!=", ">=", "<=", ">", "<", "="]
            .into_iter()
            .find(|op| tail.starts_with(*op))?;
        let mut pieces = vec![ArgPiece::Text(&tail[operator.len()..])];
        pieces.extend(pieces_of(more));
        let mut out = String::new();
        self.eval_items(&pieces, &mut out);
        let value = js_trim(&out).to_owned();
        let number = |text: &str| js_number(text);
        let compare = |test: fn(f64, f64) -> bool| match (number(&current), number(&value)) {
            (Some(left), Some(right)) => test(left, right).to_string(),
            _ => "false".to_owned(),
        };
        Some(match operator {
            "=" => {
                let written = super::paths::write(self.var(global, name), steps, &value)?;
                self.set(global, name, written);
                String::new()
            }
            "==" => (current == value).to_string(),
            "!=" => (current != value).to_string(),
            ">" => compare(|a, b| a > b),
            ">=" => compare(|a, b| a >= b),
            "<" => compare(|a, b| a < b),
            _ => compare(|a, b| a <= b),
        })
    }

    fn var(&self, global: bool, name: &str) -> Option<&String> {
        if global {
            self.globals.get(name)
        } else {
            self.vars.get(name)
        }
    }

    /// `getvarkey`: one key or index of a JSON object or array variable.
    pub(super) fn get_key(&self, global: bool, name: &str, key: &str) -> String {
        use serde_json::Value;
        let Some(stored) = self.var(global, name) else {
            return String::new();
        };
        let Ok(parsed) = serde_json::from_str::<Value>(stored) else {
            // Not JSON: SillyTavern keeps the whole value.
            return normalized(Some(stored));
        };
        let index = js_number(key);
        let found: Option<Value> = match (&parsed, index) {
            (Value::Array(items), Some(number)) => {
                js_index(number).and_then(|at| items.get(at).cloned())
            }
            (Value::Object(map), Some(number)) => map.get(&js_format(number)).cloned(),
            (Value::Object(map), None) => map.get(key).cloned(),
            (Value::String(text), Some(number)) => js_index(number).and_then(|at| {
                let units: Vec<u16> = text.encode_utf16().collect();
                units
                    .get(at)
                    .map(|unit| Value::String(String::from_utf16_lossy(&[*unit])))
            }),
            _ => None,
        };
        match found {
            None => String::new(),
            Some(Value::String(text)) => normalized(Some(&text)),
            Some(Value::Bool(flag)) => if flag { "1" } else { "0" }.to_owned(),
            Some(Value::Number(number)) => number
                .as_f64()
                .map_or_else(|| number.to_string(), js_format),
            Some(other) => other.to_string(),
        }
    }

    /// `setvarkey`: writes one key or index, creating `{}` or `[]` as the
    /// key's type asks.
    pub(super) fn set_key(&mut self, global: bool, name: &str, key: &str, value: &str) {
        use serde_json::Value;
        if name.is_empty() {
            return;
        }
        let stored = self
            .var(global, name)
            .cloned()
            .unwrap_or_else(|| "null".to_owned());
        let Ok(mut parsed) = serde_json::from_str::<Value>(&stored) else {
            return;
        };
        let index = js_number(key);
        if parsed.is_null() {
            parsed = if index.is_some() {
                Value::Array(Vec::new())
            } else {
                Value::Object(serde_json::Map::new())
            };
        }
        let value = Value::String(value.to_owned());
        match (&mut parsed, index) {
            (Value::Array(items), Some(number)) => {
                let Some(at) = js_index(number).filter(|at| (*at as f64) <= MAX_ARRAY_INDEX) else {
                    return;
                };
                if items.len() <= at {
                    items.resize(at + 1, Value::Null);
                }
                items[at] = value;
            }
            // A named key on an array is dropped by JSON.stringify.
            (Value::Array(_), None) => {}
            (Value::Object(map), Some(number)) => {
                map.insert(js_format(number), value);
            }
            (Value::Object(map), None) => {
                map.insert(key.to_owned(), value);
            }
            // Primitives cannot take keys (a TypeError in SillyTavern).
            _ => return,
        }
        self.set(global, name, parsed.to_string());
    }
}

/// A JS array index from a number, if it is one.
fn js_index(number: f64) -> Option<usize> {
    (number >= 0.0 && number.fract() == 0.0 && number < 4_294_967_295.0).then_some(number as usize)
}

/// Macros SillyTavern resolves when an `{{if}}` condition names them.
fn is_zero_arg_macro(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "user"
            | "char"
            | "input"
            | "lastusermessage"
            | "lastmessage"
            | "lastcharmessage"
            | "newline"
            | "space"
            | "noop"
    )
}

/// SillyTavern macros about the card, persona or chat that this engine
/// cannot see; `{{if description}}` and the like are left to the host.
fn is_host_name(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "description"
            | "personality"
            | "scenario"
            | "persona"
            | "mesexamples"
            | "mesexamplesraw"
            | "charprompt"
            | "charinstruction"
            | "charjailbreak"
            | "chardepthprompt"
            | "charversion"
            | "charcreatornotes"
            | "creatornotes"
            | "group"
            | "groupnotmuted"
            | "notchar"
            | "model"
            | "original"
            | "wibefore"
            | "wiafter"
            | "lorebefore"
            | "loreafter"
            | "anchorbefore"
            | "anchorafter"
            | "summary"
            | "authorsnote"
    )
}

fn raw_items(items: &[ArgPiece<'_>]) -> String {
    let mut out = String::new();
    for piece in items {
        match piece {
            ArgPiece::Text(text) => out.push_str(text),
            ArgPiece::Macro(inner) => out.push_str(&raw_text(inner)),
        }
    }
    out
}

/// The macro text as written, nested macros included.
pub(super) fn raw_text(inner: &[Node]) -> String {
    format!("{{{{{}}}}}", raw_text_inner(inner))
}

pub(super) fn raw_text_inner(inner: &[Node]) -> String {
    let mut out = String::new();
    for node in inner {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Macro(nested) => out.push_str(&raw_text(nested)),
        }
    }
    out
}
