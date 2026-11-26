//! The compact "Current state" block for prompts.

use std::fmt::Write as _;

use super::model::{Entity, Field, HelperState, Tracker, Trackers};

fn fields(fields: &[Field]) -> String {
    fields
        .iter()
        .map(|field| format!("{}: {}", field.name, field.value))
        .collect::<Vec<_>>()
        .join("; ")
}

fn entities(out: &mut String, title: &str, list: &[Entity]) {
    if list.is_empty() {
        return;
    }
    let _ = writeln!(out, "{title}:");
    for entity in list {
        let _ = writeln!(out, "- {}: {}", entity.name, fields(&entity.fields));
    }
}

/// The tracked state as one compact block, only the enabled trackers;
/// `None` when there is nothing to show. The Notebook's
/// hidden truths are included: the main prompt and the helper both need them.
#[must_use]
pub fn render_state_block(
    state: &HelperState,
    trackers: &Trackers,
    header: &str,
) -> Option<String> {
    let mut out = String::new();
    if trackers.enabled(Tracker::Scene) && !state.scene.is_empty() {
        let _ = writeln!(out, "Scene: {}", fields(&state.scene));
    }
    if trackers.enabled(Tracker::Characters) {
        entities(&mut out, "Characters", &state.characters);
    }
    if trackers.enabled(Tracker::Bonds) {
        entities(&mut out, "Bonds", &state.bonds);
    }
    if trackers.enabled(Tracker::Inventory) && !state.inventory.is_empty() {
        let _ = writeln!(out, "Inventory:");
        for holder in &state.inventory {
            let _ = writeln!(out, "- {}: {}", holder.owner, holder.items.join(", "));
        }
    }
    if trackers.enabled(Tracker::Npcs) {
        entities(&mut out, "NPCs", &state.npcs);
    }
    if trackers.enabled(Tracker::Notebook) {
        let live = state
            .notebook
            .iter()
            .filter(|t| !t.revealed)
            .collect::<Vec<_>>();
        let revealed = state
            .notebook
            .iter()
            .filter(|t| t.revealed)
            .collect::<Vec<_>>();
        if !live.is_empty() {
            let _ = writeln!(
                out,
                "Hidden truths (author's notebook; secret, reveal only when the story earns it; drop subtle breadcrumbs):"
            );
            for truth in live {
                let found = truth
                    .clues
                    .iter()
                    .filter(|c| c.found != Some(false))
                    .count();
                let mut line = format!("- {}: {}", truth.id, truth.truth);
                if !truth.hidden_from.is_empty() {
                    let _ = write!(line, " (hidden from: {})", truth.hidden_from);
                }
                let _ = write!(line, " [clues so far: {found}]");
                let _ = writeln!(out, "{line}");
            }
        }
        if !revealed.is_empty() {
            let _ = writeln!(out, "Revealed truths (now known):");
            for truth in revealed {
                let _ = writeln!(out, "- {}: {}", truth.id, truth.truth);
            }
        }
    }
    let body = out.trim_end();
    (!body.is_empty()).then(|| {
        if header.is_empty() {
            body.to_owned()
        } else {
            format!("{header}\n{body}")
        }
    })
}
