//! Fork (z1r1a3l): strips what thinking models wrap around a structured
//! fallback reply: `<think>`/`<thinking>`/`<reasoning>` blocks (closed, or an
//! orphan closing tag after leaked reasoning) and prose around a fenced block.

const TAGS: [&str; 3] = ["think", "thinking", "reasoning"];

pub(crate) fn clean(raw: &str) -> String {
    let mut text = raw.to_owned();
    for tag in TAGS {
        let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
        loop {
            let lower = text.to_ascii_lowercase();
            match (lower.find(&open), lower.find(&close)) {
                (Some(start), Some(end)) if start < end => {
                    text.replace_range(start..end + close.len(), "");
                }
                (_, Some(end)) => text.replace_range(..end + close.len(), ""),
                (Some(start), None) => {
                    // Unterminated block: keep only what follows a structured start.
                    let rest = &text[start + open.len()..];
                    match rest.find(['{', '<']) {
                        Some(offset) => text = rest[offset..].to_owned(),
                        None => text.truncate(start),
                    }
                }
                (None, None) => break,
            }
        }
    }
    let trimmed = text.trim();
    if !trimmed.starts_with("```")
        && let Some(start) = trimmed.find("```")
    {
        let body = &trimmed[start..];
        let after_open = body.find('\n').map_or(body.len(), |line| line + 1);
        let inner = &body[after_open..];
        let inner = inner.find("```").map_or(inner, |end| &inner[..end]);
        if inner.contains(['{', '<']) {
            return inner.trim().to_owned();
        }
    }
    trimmed.to_owned()
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn reasoning_and_prose_around_the_reply_are_dropped() {
        let json = r#"{"result":{"name":"no_entry","arguments":{"reason":"x"}}}"#;
        assert_eq!(
            clean(&format!("<think>use {{a}} maybe</think>\n{json}")),
            json
        );
        assert_eq!(clean(&format!("leaked thoughts {{}}</think>{json}")), json);
        assert_eq!(
            clean(&format!("Here you go:\n```json\n{json}\n```\nDone.")),
            json
        );
        assert_eq!(
            clean(&format!("<Thinking>hm</Thinking>```xml\n<a/>\n```")),
            "```xml\n<a/>\n```"
        );
        assert_eq!(clean(json), json);
    }
}
