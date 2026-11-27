//! The evaluator: SillyTavern macro semantics (its 1.13+ macro engine) for
//! the subset presets such as Vexa use.
//!
//! Text is parsed into a tree first. Macros then run in document order; a
//! macro's arguments (split at `::`) are evaluated, and trimmed, right
//! before the macro itself, so `{{setvar::a::1}}{{getvar::a}}` sees `1` and
//! `{{setvar::x::{{roll::1d20}}}}` stores the rolled number. Unknown macros
//! stay as written (with their arguments evaluated). `{{trim}}` removes the
//! line breaks around it once the whole text is done, as in SillyTavern.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::number::{js_format, js_number, js_trim};

mod blocks;
use blocks::{Form, Head, head_of, pieces_of, raw_text};

/// Variables by name.
pub type Vars = BTreeMap<String, String>;

/// Pending writes by name; `None` deletes.
pub type VarChanges = BTreeMap<String, Option<String>>;

/// One `{{roll}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiceRoll {
    /// The formula as written, e.g. `1d20`.
    pub formula: String,
    /// Each die's face.
    pub dice: Vec<u64>,
    pub modifier: i64,
    pub total: i64,
}

/// Facts the macros read; `None` names are left for the host to resolve.
#[derive(Debug, Clone, Default)]
pub struct MacroEnv {
    pub user: Option<String>,
    pub char: Option<String>,
    pub last_message: Option<String>,
    pub last_user_message: Option<String>,
    pub last_char_message: Option<String>,
    /// Seeds `{{pick}}` so the same chat keeps the same picks.
    pub pick_seed: String,
    /// The chat's own `{{key}}` macros (lower-case keys); built-in names win.
    pub custom: Vars,
}

/// The largest value a variable keeps (bytes); longer values are cut.
pub const MAX_VALUE_BYTES: usize = 256 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_MACROS_PER_RUN: usize = 200_000;
const MAX_DICE: u64 = 1000;

#[derive(Debug)]
enum Node {
    Text(String),
    Macro(Vec<Node>),
}

fn parse(text: &str) -> Vec<Node> {
    struct Frame {
        nodes: Vec<Node>,
    }
    fn push_text(frame: &mut Frame, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(Node::Text(last)) = frame.nodes.last_mut() {
            last.push_str(text);
        } else {
            frame.nodes.push(Node::Text(text.to_owned()));
        }
    }
    let bytes = text.as_bytes();
    let mut stack = vec![Frame { nodes: Vec::new() }];
    let (mut index, mut start) = (0, 0);
    while index + 1 < bytes.len() {
        if bytes[index] == b'{' && bytes[index + 1] == b'{' && stack.len() <= MAX_DEPTH {
            let top = stack.last_mut().expect("root frame");
            push_text(top, &text[start..index]);
            stack.push(Frame { nodes: Vec::new() });
            index += 2;
            start = index;
        } else if bytes[index] == b'}' && bytes[index + 1] == b'}' && stack.len() > 1 {
            let mut frame = stack.pop().expect("open frame");
            push_text(&mut frame, &text[start..index]);
            stack
                .last_mut()
                .expect("root frame")
                .nodes
                .push(Node::Macro(frame.nodes));
            index += 2;
            start = index;
        } else {
            index += 1;
        }
    }
    push_text(stack.last_mut().expect("root frame"), &text[start..]);
    // Unclosed `{{`: plain text again.
    while stack.len() > 1 {
        let frame = stack.pop().expect("open frame");
        let top = stack.last_mut().expect("root frame");
        push_text(top, "{{");
        for node in frame.nodes {
            match node {
                Node::Text(text) => push_text(top, &text),
                node @ Node::Macro(_) => top.nodes.push(node),
            }
        }
    }
    stack.pop().expect("root frame").nodes
}

/// One argument: text pieces and nested macros, in order.
#[derive(Debug, Clone, Copy)]
enum ArgPiece<'a> {
    Text(&'a str),
    Macro(&'a [Node]),
}

fn args_of(nodes: &[Node]) -> Vec<Vec<ArgPiece<'_>>> {
    split_pieces(&pieces_of(nodes))
}

/// Splits pieces into arguments at each `::` outside nested macros.
fn split_pieces<'a>(pieces: &[ArgPiece<'a>]) -> Vec<Vec<ArgPiece<'a>>> {
    let mut out: Vec<Vec<ArgPiece<'a>>> = vec![Vec::new()];
    for piece in pieces {
        match piece {
            ArgPiece::Text(text) => {
                let mut parts = text.split("::");
                if let Some(first) = parts.next() {
                    out.last_mut().expect("segment").push(ArgPiece::Text(first));
                }
                for part in parts {
                    out.push(vec![ArgPiece::Text(part)]);
                }
            }
            ArgPiece::Macro(inner) => out
                .last_mut()
                .expect("segment")
                .push(ArgPiece::Macro(inner)),
        }
    }
    out
}

/// A small xorshift generator; each run seeds its own.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn fresh() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        let mixed = time ^ COUNTER.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
        Self(splitmix(mixed) | 1)
    }

    fn below(&mut self, bound: u64) -> u64 {
        if bound <= 1 {
            return 0;
        }
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        // Multiply-shift keeps the result unbiased enough for dice.
        ((u128::from(splitmix(x)) * u128::from(bound)) >> 64) as u64
    }
}

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash ^= 0x1f;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// A stable fingerprint of a prompt's texts (FNV-1a 64, hex), used to find
/// a reply's dice again from its recorded request.
#[must_use]
pub fn context_fingerprint<'a>(texts: impl IntoIterator<Item = &'a str>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for text in texts {
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash ^= 0x1f;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Parses `NdM`, `dM`, `M`, each with an optional `+K` / `-K`.
fn parse_formula(formula: &str) -> Option<(u64, u64, i64)> {
    let compact: String = formula
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if compact.is_empty() {
        return None;
    }
    let compact = if compact.chars().all(|c| c.is_ascii_digit()) {
        format!("1d{compact}")
    } else {
        compact
    };
    let (count, rest) = compact.split_once('d')?;
    let count: u64 = if count.is_empty() {
        1
    } else {
        count.parse().ok()?
    };
    let (sides, modifier) = match rest.find(['+', '-']) {
        Some(at) => (&rest[..at], rest[at..].parse::<i64>().ok()?),
        None => (rest, 0),
    };
    let sides: u64 = sides.parse().ok()?;
    if count == 0 || count > MAX_DICE || sides == 0 || sides > 1_000_000_000 {
        return None;
    }
    Some((count, sides, modifier))
}

/// One evaluation session: a generation, a preview, or a regex replacement.
#[derive(Debug)]
pub struct MacroRun {
    env: MacroEnv,
    vars: Vars,
    globals: Vars,
    chat_changes: VarChanges,
    global_changes: VarChanges,
    rolls: Vec<DiceRoll>,
    rng: Rng,
    picks: HashMap<String, u64>,
    evaluated: usize,
}

impl MacroRun {
    #[must_use]
    pub fn new(env: MacroEnv, vars: Vars, globals: Vars) -> Self {
        Self {
            env,
            vars,
            globals,
            chat_changes: VarChanges::new(),
            global_changes: VarChanges::new(),
            rolls: Vec::new(),
            rng: Rng::fresh(),
            picks: HashMap::new(),
            evaluated: 0,
        }
    }

    /// A fixed random sequence, for tests.
    #[must_use]
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = Rng(splitmix(seed) | 1);
        self
    }

    #[must_use]
    pub fn vars(&self) -> &Vars {
        &self.vars
    }

    #[must_use]
    pub fn globals(&self) -> &Vars {
        &self.globals
    }

    #[must_use]
    pub fn chat_changes(&self) -> &VarChanges {
        &self.chat_changes
    }

    #[must_use]
    pub fn global_changes(&self) -> &VarChanges {
        &self.global_changes
    }

    #[must_use]
    pub fn rolls(&self) -> &[DiceRoll] {
        &self.rolls
    }

    /// Evaluates `text`; a text without `{{` comes back unchanged.
    pub fn evaluate(&mut self, text: &str) -> String {
        if !text.contains("{{") {
            return text.to_owned();
        }
        let nodes = parse(text);
        let mut out = String::with_capacity(text.len());
        self.eval_nodes(&nodes, &mut out);
        legacy_trim(&out)
    }

    fn eval_nodes(&mut self, nodes: &[Node], out: &mut String) {
        self.eval_items(&pieces_of(nodes), out);
    }

    fn eval_macro(&mut self, inner: &[Node]) -> String {
        if let Some(Node::Text(head)) = inner.first()
            && js_trim(head).starts_with("//")
        {
            return String::new();
        }
        self.evaluated += 1;
        if self.evaluated > MAX_MACROS_PER_RUN {
            let mut body = String::new();
            self.eval_nodes(inner, &mut body);
            return format!("{{{{{body}}}}}");
        }
        match head_of(inner) {
            Head::Named(header) if !header.closing => {
                if header.name == "if" {
                    // `{{if c}}` without its `{{/if}}` stays as written.
                    if header.args.len() != 2 {
                        return raw_text(inner);
                    }
                    return self
                        .eval_if(&header, None)
                        .unwrap_or_else(|| raw_text(inner));
                }
                let raw: Vec<String> = header
                    .args
                    .iter()
                    .map(|pieces| {
                        let mut value = String::new();
                        self.eval_items(pieces, &mut value);
                        value
                    })
                    .collect();
                let args: Vec<String> = raw.iter().map(|value| js_trim(value).to_owned()).collect();
                if let Some(value) = self.resolve(&header.name, &args) {
                    return value;
                }
                if header.form == Form::Colons {
                    return Self::unknown(&args_of(inner), &raw);
                }
                let mut body = String::new();
                self.eval_nodes(inner, &mut body);
                format!("{{{{{body}}}}}")
            }
            Head::Shorthand(global, after, more) => self.eval_shorthand(global, after, more, inner),
            _ => {
                // Legacy one-colon forms: `{{random:a,b}}`, `{{pick: a, b}}`.
                let mut body = String::new();
                self.eval_nodes(inner, &mut body);
                if let Some(value) = self.resolve_legacy(&body) {
                    return value;
                }
                format!("{{{{{body}}}}}")
            }
        }
    }

    fn unknown(segments: &[Vec<ArgPiece<'_>>], args: &[String]) -> String {
        let mut out = String::from("{{");
        if let Some([ArgPiece::Text(head)]) = segments.first().map(Vec::as_slice) {
            out.push_str(head);
        }
        for arg in args {
            out.push_str("::");
            out.push_str(arg);
        }
        out.push_str("}}");
        out
    }

    fn resolve_legacy(&mut self, body: &str) -> Option<String> {
        let body = js_trim(body);
        let split = body.find(|c: char| c == ':' || c.is_whitespace())?;
        let name = body[..split].to_lowercase();
        let rest = body[split..].trim_start();
        let rest = rest.strip_prefix(':').unwrap_or(rest);
        let list = || -> Vec<String> {
            if rest.contains("::") {
                return rest
                    .split("::")
                    .map(|item| js_trim(item).to_owned())
                    .collect();
            }
            rest.replace("\\,", "\u{0}")
                .split(',')
                .map(|item| js_trim(item).replace('\u{0}', ","))
                .collect()
        };
        match name.as_str() {
            "random" => Some(self.random(&list())),
            "pick" => Some(self.pick(body, &list())),
            "roll" => Some(self.roll(js_trim(rest))),
            _ => None,
        }
    }

    fn random(&mut self, list: &[String]) -> String {
        if list.is_empty() {
            return String::new();
        }
        let at = self.rng.below(list.len() as u64) as usize;
        list[at].clone()
    }

    /// The same pick each time for the same chat, text and occurrence.
    fn pick(&mut self, raw: &str, list: &[String]) -> String {
        if list.is_empty() {
            return String::new();
        }
        let count = self.picks.entry(raw.to_owned()).or_insert(0);
        let occurrence = *count;
        *count += 1;
        let hash = fnv1a(&[
            self.env.pick_seed.as_bytes(),
            raw.as_bytes(),
            &occurrence.to_le_bytes(),
        ]);
        list[(splitmix(hash) % list.len() as u64) as usize].clone()
    }

    fn roll(&mut self, formula: &str) -> String {
        let Some((count, sides, modifier)) = parse_formula(formula) else {
            return String::new();
        };
        let dice: Vec<u64> = (0..count).map(|_| self.rng.below(sides) + 1).collect();
        let sum: i64 = dice.iter().map(|d| *d as i64).sum();
        let total = sum.saturating_add(modifier);
        self.rolls.push(DiceRoll {
            formula: formula.to_owned(),
            dice,
            modifier,
            total,
        });
        total.to_string()
    }

    fn resolve(&mut self, name: &str, args: &[String]) -> Option<String> {
        let first = || args.first().cloned().unwrap_or_default();
        let rest = || {
            args.get(1..)
                .map(|rest| rest.join("::"))
                .unwrap_or_default()
        };
        let arg = |at: usize| args.get(at).cloned().unwrap_or_default();
        let tail = |at: usize| {
            args.get(at..)
                .map(|rest| rest.join("::"))
                .unwrap_or_default()
        };
        Some(match name {
            "user" => self.env.user.clone()?,
            "char" => self.env.char.clone()?,
            "noop" => String::new(),
            "newline" => "\n".to_owned(),
            "space" => " ".to_owned(),
            // Scoped `{{trim}}…{{/trim}}` returns its (already trimmed) body.
            "trim" => args
                .first()
                .cloned()
                .unwrap_or_else(|| "{{trim}}".to_owned()),
            "comment" => String::new(),
            // One argument is a legacy comma list: `{{random::a,b}}`.
            "random" => {
                let list = single_list(args);
                self.random(&list)
            }
            "pick" => {
                let raw = format!("pick::{}", args.join("::"));
                let list = single_list(args);
                self.pick(&raw, &list)
            }
            "roll" => self.roll(&first()),
            "reverse" => args.join("::").chars().rev().collect(),
            "input" | "lastusermessage" => self.env.last_user_message.clone().unwrap_or_default(),
            "lastmessage" => self.env.last_message.clone().unwrap_or_default(),
            "lastcharmessage" => self.env.last_char_message.clone().unwrap_or_default(),
            "getvar" => normalized(self.vars.get(&first())),
            "getglobalvar" => normalized(self.globals.get(&first())),
            "setvar" => {
                self.set(false, &first(), rest());
                String::new()
            }
            "setglobalvar" => {
                self.set(true, &first(), rest());
                String::new()
            }
            "addvar" => {
                self.add(false, &first(), &rest());
                String::new()
            }
            "addglobalvar" => {
                self.add(true, &first(), &rest());
                String::new()
            }
            "incvar" => self.add(false, &first(), "1"),
            "decvar" => self.add(false, &first(), "-1"),
            "incglobalvar" => self.add(true, &first(), "1"),
            "decglobalvar" => self.add(true, &first(), "-1"),
            "hasvar" | "varexists" => self.vars.contains_key(&first()).to_string(),
            "hasglobalvar" | "globalvarexists" => self.globals.contains_key(&first()).to_string(),
            "deletevar" | "flushvar" => {
                self.delete(false, &first());
                String::new()
            }
            "deleteglobalvar" | "flushglobalvar" => {
                self.delete(true, &first());
                String::new()
            }
            "getvarkey" | "getvarindex" => self.get_key(false, &first(), &arg(1)),
            "getglobalvarkey" | "getglobalvarindex" => self.get_key(true, &first(), &arg(1)),
            "setvarkey" | "setvarindex" => {
                self.set_key(false, &first(), &arg(1), &tail(2));
                String::new()
            }
            "setglobalvarkey" | "setglobalvarindex" => {
                self.set_key(true, &first(), &arg(1), &tail(2));
                String::new()
            }
            // Only meaningful inside `{{if}}…{{/if}}`.
            "else" => String::new(),
            _ if args.is_empty() => return self.env.custom.get(&name.to_lowercase()).cloned(),
            _ => return None,
        })
    }

    fn set(&mut self, global: bool, name: &str, mut value: String) {
        if name.is_empty() {
            return;
        }
        if value.len() > MAX_VALUE_BYTES {
            let mut cut = MAX_VALUE_BYTES;
            while !value.is_char_boundary(cut) {
                cut -= 1;
            }
            value.truncate(cut);
        }
        let (vars, changes) = if global {
            (&mut self.globals, &mut self.global_changes)
        } else {
            (&mut self.vars, &mut self.chat_changes)
        };
        vars.insert(name.to_owned(), value.clone());
        changes.insert(name.to_owned(), Some(value));
    }

    fn delete(&mut self, global: bool, name: &str) {
        let (vars, changes) = if global {
            (&mut self.globals, &mut self.global_changes)
        } else {
            (&mut self.vars, &mut self.chat_changes)
        };
        if vars.remove(name).is_some() {
            changes.insert(name.to_owned(), None);
        }
    }

    /// SillyTavern's `addLocalVariable`: push onto a JSON array, add two
    /// numbers, or append text. Returns the new value.
    fn add(&mut self, global: bool, name: &str, value: &str) -> String {
        if name.is_empty() {
            return String::new();
        }
        let stored = if global {
            self.globals.get(name)
        } else {
            self.vars.get(name)
        };
        // `getLocalVariable(name) || 0`: missing, empty and zero read as 0.
        let current: Result<f64, String> = match stored {
            None => Ok(0.0),
            Some(text) if text.is_empty() => Ok(0.0),
            Some(text) if js_trim(text).is_empty() => Err(text.clone()),
            Some(text) => js_number(text).ok_or_else(|| text.clone()),
        };
        if let Err(text) = &current
            && let Ok(serde_json::Value::Array(mut items)) = serde_json::from_str(text)
        {
            items.push(serde_json::Value::String(value.to_owned()));
            let next = serde_json::Value::Array(items).to_string();
            self.set(global, name, next.clone());
            return next;
        }
        let base = match &current {
            Ok(number) => Some(*number),
            Err(text) => js_number(text),
        };
        let next = match (js_number(value), base) {
            (Some(increment), Some(base)) => {
                let sum = base + increment;
                if sum.is_nan() {
                    return String::new();
                }
                js_format(sum)
            }
            _ => {
                let prefix = match &current {
                    Ok(number) if *number == 0.0 => String::new(),
                    Ok(number) => js_format(*number),
                    Err(text) => text.clone(),
                };
                format!("{prefix}{value}")
            }
        };
        self.set(global, name, next.clone());
        next
    }
}

/// `readSingleArgsRandomList` for a one-argument list.
fn single_list(args: &[String]) -> Vec<String> {
    match args {
        [only] => only
            .replace("\\,", "\u{0}")
            .split(',')
            .map(|item| js_trim(item).replace('\u{0}', ","))
            .collect(),
        _ => args.to_vec(),
    }
}

/// `getLocalVariable`: a numeric value reads back as a JS number.
fn normalized(value: Option<&String>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if js_trim(value).is_empty() {
        return value.clone();
    }
    match js_number(value) {
        Some(number) => js_format(number),
        None => value.clone(),
    }
}

/// `/(?:\r?\n)*{{trim}}(?:\r?\n)*/gi` → "".
fn legacy_trim(text: &str) -> String {
    const MARK: &str = "{{trim}}";
    let lower = text.to_ascii_lowercase();
    if !lower.contains(MARK) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    let mut search = 0;
    while let Some(found) = lower[search..].find(MARK) {
        let at = search + found;
        let mut start = at;
        let bytes = text.as_bytes();
        // Eat `\r?\n` pairs before the mark (only those after `rest`).
        loop {
            if start >= rest + 1 && bytes[start - 1] == b'\n' {
                start -= 1;
                if start > rest && bytes[start - 1] == b'\r' {
                    start -= 1;
                }
            } else {
                break;
            }
        }
        out.push_str(&text[rest..start]);
        let mut end = at + MARK.len();
        loop {
            if bytes.get(end) == Some(&b'\n') {
                end += 1;
            } else if bytes.get(end) == Some(&b'\r') && bytes.get(end + 1) == Some(&b'\n') {
                end += 2;
            } else {
                break;
            }
        }
        rest = end;
        search = end;
    }
    out.push_str(&text[rest..]);
    out
}
