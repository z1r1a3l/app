//! Shared helpers for the hub adapters: request descriptions, JSON field
//! readers, HTML-to-text, and error constructors.

use lettuce_contracts::{ApiError, ApiErrorCode};
use serde_json::Value;

/// One HTTP request an adapter wants made. Adapters only describe requests
/// and parse bodies, so they stay pure and testable with recorded fixtures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpRequest {
    pub url: String,
    pub query: Vec<(String, String)>,
    /// `Some` makes it a POST with this JSON body.
    pub body: Option<String>,
    pub headers: Vec<(&'static str, &'static str)>,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self { url: url.into(), query: Vec::new(), body: None, headers: vec![("accept", "application/json")] }
    }

    /// A binary download (image or card file). Image CDNs that negotiate
    /// formats are steered away from AVIF, which the card writer cannot read.
    pub fn file(url: impl Into<String>) -> Self {
        Self { url: url.into(), query: Vec::new(), body: None, headers: vec![("accept", "image/png,image/webp,image/jpeg,application/json;q=0.9,*/*;q=0.5")] }
    }

    pub fn param(mut self, name: &str, value: impl ToString) -> Self {
        self.query.push((name.to_owned(), value.to_string()));
        self
    }

    pub fn param_if(self, condition: bool, name: &str, value: impl ToString) -> Self {
        if condition { self.param(name, value) } else { self }
    }
}

pub(crate) fn error(code: ApiErrorCode, message: impl Into<String>) -> ApiError {
    ApiError { code, message: message.into(), details: None }
}

pub(crate) fn malformed(source: &str, what: &str) -> ApiError {
    error(ApiErrorCode::Malformed, format!("{source} returned an unexpected {what}; the site may have changed"))
}

pub(crate) fn unsupported(message: impl Into<String>) -> ApiError {
    error(ApiErrorCode::Unsupported, message)
}

pub(crate) fn json(bytes: &[u8], source: &str, what: &str) -> Result<Value, ApiError> {
    serde_json::from_slice(bytes).map_err(|_| malformed(source, what))
}

pub(crate) fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned()
}

pub(crate) fn opt_text(value: &Value, key: &str) -> Option<String> {
    Some(text(value, key)).filter(|text| !text.is_empty())
}

/// A count that may arrive as a number or a numeric string (protobuf JSON
/// encodes 64-bit ints as strings).
pub(crate) fn count(value: &Value, key: &str) -> Option<u32> {
    match value.get(key)? {
        Value::Number(number) => number.as_f64().map(|n| n.max(0.0).min(f64::from(u32::MAX)) as u32),
        Value::String(text) => text.trim().parse::<f64>().ok().map(|n| n.max(0.0).min(f64::from(u32::MAX)) as u32),
        _ => None,
    }
}

pub(crate) fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    Value::String(text) => Some(text.trim().to_owned()),
                    Value::Object(_) => ["name", "label", "title"].iter().find_map(|key| item.get(*key).and_then(Value::as_str)).map(|t| t.trim().to_owned()),
                    _ => None,
                })
                .filter(|text| !text.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn array_len(value: &Value, key: &str) -> u32 {
    value.get(key).and_then(Value::as_array).map_or(0, |items| {
        u32::try_from(items.iter().filter(|item| item.as_str().is_none_or(|text| !text.trim().is_empty())).count()).unwrap_or(u32::MAX)
    })
}

/// Joins a site-relative path onto its origin.
pub(crate) fn absolute(origin: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") { path.to_owned() } else { format!("{origin}{path}") }
}

/// Plain text from the light HTML hubs use for descriptions: block tags
/// become line breaks, other tags are dropped, common entities decoded.
pub(crate) fn html_to_text(html: &str) -> String {
    if !html.contains('<') && !html.contains('&') {
        return html.trim().to_owned();
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let tag = rest[start + 1..start + end].trim_start_matches('/').to_ascii_lowercase();
        let name: String = tag.chars().take_while(char::is_ascii_alphanumeric).collect();
        if matches!(name.as_str(), "br" | "p" | "div" | "li" | "hr" | "h1" | "h2" | "h3" | "h4" | "tr") {
            out.push('\n');
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&");
    let mut lines: Vec<&str> = Vec::new();
    for line in decoded.lines().map(str::trim_end) {
        if line.trim().is_empty() && lines.last().is_none_or(|last| last.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_owned()
}

/// The first `limit` characters of a text, cut at a character boundary.
pub(crate) fn clip(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    match trimmed.char_indices().nth(limit) {
        Some((index, _)) => format!("{}…", trimmed[..index].trim_end()),
        None => trimmed.to_owned(),
    }
}

/// A safe file name for a download: letters, digits, `-` and `_`.
pub(crate) fn file_name(name: &str, extension: &str) -> String {
    let mut stem: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(60)
        .collect();
    if stem.is_empty() {
        stem = "card".to_owned();
    }
    format!("{stem}.{extension}")
}

/// Page-number cursors for hubs that page by number.
pub(crate) fn page_of(cursor: Option<&str>) -> u32 {
    cursor.and_then(|cursor| cursor.parse::<u32>().ok()).filter(|page| *page >= 1).unwrap_or(1)
}

pub(crate) fn next_page(page: u32, has_more: bool) -> Option<String> {
    has_more.then(|| (page + 1).to_string())
}

pub(crate) fn comma_list(tags: &[String]) -> String {
    tags.iter().map(|tag| tag.trim()).filter(|tag| !tag.is_empty()).collect::<Vec<_>>().join(",")
}
