//! Fork-only: AetherKit SDK shorthand adapter (Phase 3, Option B).
//!
//! A card can declare flags, hooks, arc/track specs, a state block and custom
//! dice with `[[…]]` shorthand. SillyTavern is supposed to convert that into
//! the empty `flagN` / `hookCallN` / `pluginArcSpec` / `pluginTracksSpec` /
//! `pluginStateTemplate` / `pluginDiceN` variables that Vexa's System Vars
//! block declares. The official conversion regex is not published, so this
//! adapter does the conversion deterministically from the SDK examples.
//!
//! If a card (or another script in the chat's scope) already ships a
//! conversion regex, that pack wins (Option A) and this adapter stays out.
//!
//! [`KEEP_SPEC_BODIES_VISIBLE`] keeps authored spec, state and dice bodies
//! in the prompt for Vexa v1.0; flip it off when a newer preset getvars
//! those slots on its own.

mod parser;

/// When true, `[[spec]]` / `[[state]]` / `[[dice]]` bodies stay visible in
/// the card text (via `{{getvar}}` after the matching `{{setvar}}`) because
/// public Vexa v1.0 never reads those plugin slots. Set to `false` for v2
/// once the preset injects them itself. Flags and hooks are always stripped.
pub const KEEP_SPEC_BODIES_VISIBLE: bool = true;

pub use parser::{Declaration, adapt_text, adapt_text_with, declarations, setvar};

use crate::fork_regex::{RegexScript, RegexTarget, run_regex_script};

/// A script that already converts AetherKit shorthand (Option A).
#[must_use]
pub fn is_conversion_script(script: &RegexScript) -> bool {
    if script.disabled {
        return false;
    }
    if !script.applies_to(RegexTarget::Prompt) && !script.applies_to(RegexTarget::Display) {
        return false;
    }
    let find = unescaped(&script.find_regex).to_ascii_lowercase();
    if find.contains("[[flag")
        || find.contains("[[hook")
        || find.contains("[[spec")
        || find.contains("[[state")
        || find.contains("[[dice")
    {
        return true;
    }
    let replace = script.replace_string.to_ascii_lowercase();
    replace.contains("setvar::flag")
        || replace.contains("setvar::hookcall")
        || replace.contains("setvar::pluginarcspec")
        || replace.contains("setvar::plugintracksspec")
        || replace.contains("setvar::pluginstatetemplate")
        || replace.contains("setvar::plugindice")
}

/// True when any script in the chat's effective list is a converter.
#[must_use]
pub fn has_conversion_script<'a, I>(scripts: I) -> bool
where
    I: IntoIterator<Item = &'a RegexScript>,
{
    scripts.into_iter().any(is_conversion_script)
}

/// Runs Option A converters over card/lorebook/preset text. Placement and
/// depth are ignored: the official pack's placement is unknown, and these
/// scripts exist to rewrite shorthand wherever it was written.
#[must_use]
pub fn apply_conversion_scripts<'a, I, M>(scripts: I, text: &str, macros: &M) -> String
where
    I: IntoIterator<Item = &'a RegexScript>,
    M: Fn(&str) -> String + ?Sized,
{
    let mut out = text.to_owned();
    if out.is_empty() {
        return out;
    }
    for script in scripts {
        if is_conversion_script(script) && script.applies_to(RegexTarget::Prompt) {
            out = run_regex_script(script, &out, macros);
        }
    }
    out
}

/// Rewrite one non-history prompt text: Option A converters if present,
/// otherwise the built-in adapter.
#[must_use]
pub fn rewrite_prompt_text<'a, I, M>(scripts: I, text: &str, macros: &M) -> String
where
    I: IntoIterator<Item = &'a RegexScript>,
    M: Fn(&str) -> String + ?Sized,
{
    let scripts: Vec<&RegexScript> = scripts.into_iter().collect();
    if has_conversion_script(scripts.iter().copied()) {
        apply_conversion_scripts(scripts, text, macros)
    } else {
        adapt_text(text)
    }
}

fn unescaped(pattern: &str) -> String {
    // `/\[\[flag1:` and `[[flag1:` should both count as converters.
    pattern.replace('\\', "")
}

#[cfg(test)]
mod tests;
