//! JavaScript `Number(...)` parsing and `String(number)` formatting, so
//! variable arithmetic matches SillyTavern byte for byte.

fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `String.prototype.trim`.
pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// `Number(text)`; `None` stands for `NaN`.
pub(crate) fn js_number(text: &str) -> Option<f64> {
    let text = js_trim(text);
    if text.is_empty() {
        return Some(0.0);
    }
    match text {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let lower = text.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = lower.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return None;
            }
            return Some(digits.chars().fold(0.0, |acc, c| {
                acc * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
            }));
        }
    }
    // A decimal literal: optional sign, digits with an optional dot, an
    // optional exponent. Rust's parser also takes "inf"/"nan", JS does not.
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let digits = mantissa.replacen('.', "", 1);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    text.parse::<f64>().ok()
}

/// `String(number)`.
pub(crate) fn js_format(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    let magnitude = value.abs();
    if (1e-6..1e21).contains(&magnitude) {
        let text = format!("{value}");
        return text;
    }
    // Exponent form: "1e+21", "1.5e-7".
    let text = format!("{value:e}");
    match text.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') => {
            format!("{mantissa}e+{exponent}")
        }
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_javascript() {
        assert_eq!(js_number(""), Some(0.0));
        assert_eq!(js_number("  12 "), Some(12.0));
        assert_eq!(js_number("1.5"), Some(1.5));
        assert_eq!(js_number(".5"), Some(0.5));
        assert_eq!(js_number("5."), Some(5.0));
        assert_eq!(js_number("-3"), Some(-3.0));
        assert_eq!(js_number("1e3"), Some(1000.0));
        assert_eq!(js_number("0x10"), Some(16.0));
        assert_eq!(js_number("Infinity"), Some(f64::INFINITY));
        assert_eq!(js_number("inf"), None);
        assert_eq!(js_number("nan"), None);
        assert_eq!(js_number("abc"), None);
        assert_eq!(js_number("1,5"), None);
        assert_eq!(js_number("∅"), None);
        assert_eq!(js_number("-0x10"), None);
    }

    #[test]
    fn formats_like_javascript() {
        assert_eq!(js_format(6.0), "6");
        assert_eq!(js_format(-1.0), "-1");
        assert_eq!(js_format(2.5), "2.5");
        assert_eq!(js_format(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(js_format(1e21), "1e+21");
        assert_eq!(js_format(-0.0), "0");
    }
}
