//! The tracked state, the change operations and how they apply.

use serde::{Deserialize, Serialize};

/// One tracker the helper can keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tracker {
    /// World state and scene: time, place, weather, situation.
    Scene,
    /// Characters' outfits, locations, poses and conditions.
    Characters,
    /// Relationships and bonds towards the user.
    Bonds,
    /// Who carries what.
    Inventory,
    /// NPC sheet: role, look, status, attitude.
    Npcs,
    /// Hidden world/NPC truths with breadcrumbs.
    Notebook,
}

impl Tracker {
    pub const ALL: [Self; 6] = [
        Self::Scene,
        Self::Characters,
        Self::Bonds,
        Self::Inventory,
        Self::Npcs,
        Self::Notebook,
    ];

    /// The section tag the helper writes (`[Scene]`).
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Scene => "Scene",
            Self::Characters => "Characters",
            Self::Bonds => "Bonds",
            Self::Inventory => "Inventory",
            Self::Npcs => "NPC",
            Self::Notebook => "Notebook",
        }
    }
}

/// Which trackers run; every one is on unless switched off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Trackers {
    pub scene: bool,
    pub characters: bool,
    pub bonds: bool,
    pub inventory: bool,
    pub npcs: bool,
    pub notebook: bool,
}

impl Default for Trackers {
    fn default() -> Self {
        Self {
            scene: true,
            characters: true,
            bonds: true,
            inventory: true,
            npcs: true,
            notebook: true,
        }
    }
}

impl Trackers {
    #[must_use]
    pub const fn enabled(&self, tracker: Tracker) -> bool {
        match tracker {
            Tracker::Scene => self.scene,
            Tracker::Characters => self.characters,
            Tracker::Bonds => self.bonds,
            Tracker::Inventory => self.inventory,
            Tracker::Npcs => self.npcs,
            Tracker::Notebook => self.notebook,
        }
    }

    #[must_use]
    pub fn list(&self) -> Vec<Tracker> {
        Tracker::ALL
            .into_iter()
            .filter(|tracker| self.enabled(*tracker))
            .collect()
    }

    #[must_use]
    pub fn any(&self) -> bool {
        !self.list().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Entity {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Holder {
    pub owner: String,
    #[serde(default)]
    pub items: Vec<String>,
}

/// A breadcrumb the story dropped for a truth.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Clue {
    pub text: String,
    /// The reply the clue was claimed for.
    #[serde(default)]
    pub message_id: String,
    /// Whether the clue's words were found in that reply; `None` when it
    /// was not checked (a manual clue).
    #[serde(default)]
    pub found: Option<bool>,
}

/// A hidden truth about the world or an NPC (never the user's persona).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Truth {
    pub id: String,
    pub truth: String,
    #[serde(default)]
    pub hidden_from: String,
    #[serde(default)]
    pub revealed: bool,
    #[serde(default)]
    pub clues: Vec<Clue>,
}

/// The full tracked state of one chat path.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HelperState {
    pub scene: Vec<Field>,
    pub characters: Vec<Entity>,
    pub bonds: Vec<Entity>,
    pub inventory: Vec<Holder>,
    pub npcs: Vec<Entity>,
    pub notebook: Vec<Truth>,
}

impl HelperState {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scene.is_empty()
            && self.characters.is_empty()
            && self.bonds.is_empty()
            && self.inventory.is_empty()
            && self.npcs.is_empty()
            && self.notebook.is_empty()
    }

    fn entities_mut(&mut self, tracker: Tracker) -> Option<&mut Vec<Entity>> {
        match tracker {
            Tracker::Characters => Some(&mut self.characters),
            Tracker::Bonds => Some(&mut self.bonds),
            Tracker::Npcs => Some(&mut self.npcs),
            _ => None,
        }
    }

    #[must_use]
    pub fn entities(&self, tracker: Tracker) -> Option<&[Entity]> {
        match tracker {
            Tracker::Characters => Some(&self.characters),
            Tracker::Bonds => Some(&self.bonds),
            Tracker::Npcs => Some(&self.npcs),
            _ => None,
        }
    }
}

/// One change. Values are whatever the helper wrote (Russian by default).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Sets one field; an empty value removes it. Scene fields have no
    /// entity (empty string).
    Set {
        tracker: Tracker,
        #[serde(default)]
        entity: String,
        field: String,
        value: String,
    },
    /// Drops a whole character, bond, NPC or inventory holder.
    Remove {
        tracker: Tracker,
        entity: String,
    },
    ItemAdd {
        owner: String,
        item: String,
    },
    ItemRemove {
        owner: String,
        item: String,
    },
    /// Adds a truth, or rewrites the one with this id (its clues stay).
    Truth {
        id: String,
        truth: String,
        #[serde(default)]
        hidden_from: String,
    },
    Clue {
        id: String,
        text: String,
        #[serde(default)]
        found: Option<bool>,
    },
    Reveal {
        id: String,
        #[serde(default = "yes")]
        revealed: bool,
    },
    TruthRemove {
        id: String,
    },
}

const fn yes() -> bool {
    true
}

impl Op {
    #[must_use]
    pub const fn tracker(&self) -> Tracker {
        match self {
            Self::Set { tracker, .. } | Self::Remove { tracker, .. } => *tracker,
            Self::ItemAdd { .. } | Self::ItemRemove { .. } => Tracker::Inventory,
            Self::Truth { .. }
            | Self::Clue { .. }
            | Self::Reveal { .. }
            | Self::TruthRemove { .. } => Tracker::Notebook,
        }
    }
}

/// The changes one reply (or one manual edit) made.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StateDelta {
    /// The reply the changes belong to; empty for chat-wide base edits.
    #[serde(default)]
    pub message_id: String,
    pub ops: Vec<Op>,
}

/// How names, labels and items are compared: case, quotes, markdown and
/// spacing do not count; `ё` reads as `е`.
#[must_use]
pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '"' | '\'' | '“' | '”' | '«' | '»' | '*' | '_' | '`'))
        .flat_map(char::to_lowercase)
        .map(|c| if c == 'ё' { 'е' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[must_use]
pub fn same_key(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

fn set_field(fields: &mut Vec<Field>, name: &str, value: &str) {
    let position = fields.iter().position(|field| same_key(&field.name, name));
    match (position, value.trim().is_empty()) {
        (Some(index), true) => {
            fields.remove(index);
        }
        (Some(index), false) => {
            value.trim().clone_into(&mut fields[index].value);
        }
        (None, true) => {}
        (None, false) => fields.push(Field {
            name: name.trim().to_owned(),
            value: value.trim().to_owned(),
        }),
    }
}

fn truth_mut<'a>(notebook: &'a mut [Truth], id: &str) -> Option<&'a mut Truth> {
    notebook.iter_mut().find(|truth| same_key(&truth.id, id))
}

/// Applies `ops` in order; `message_id` is stamped on new clues.
pub fn apply(state: &mut HelperState, ops: &[Op], message_id: &str) {
    for op in ops {
        match op {
            Op::Set {
                tracker: Tracker::Scene,
                field,
                value,
                ..
            } => set_field(&mut state.scene, field, value),
            Op::Set {
                tracker,
                entity,
                field,
                value,
            } => {
                let Some(entities) = state.entities_mut(*tracker) else {
                    continue;
                };
                if entity.trim().is_empty() || field.trim().is_empty() {
                    continue;
                }
                let index = match entities.iter().position(|e| same_key(&e.name, entity)) {
                    Some(index) => index,
                    None if value.trim().is_empty() => continue,
                    None => {
                        entities.push(Entity {
                            name: entity.trim().to_owned(),
                            fields: Vec::new(),
                        });
                        entities.len() - 1
                    }
                };
                set_field(&mut entities[index].fields, field, value);
                if entities[index].fields.is_empty() {
                    entities.remove(index);
                }
            }
            Op::Remove { tracker, entity } => match tracker {
                Tracker::Inventory => state.inventory.retain(|h| !same_key(&h.owner, entity)),
                Tracker::Scene => state.scene.retain(|f| !same_key(&f.name, entity)),
                Tracker::Notebook => state.notebook.retain(|t| !same_key(&t.id, entity)),
                _ => {
                    if let Some(entities) = state.entities_mut(*tracker) {
                        entities.retain(|e| !same_key(&e.name, entity));
                    }
                }
            },
            Op::ItemAdd { owner, item } => {
                if owner.trim().is_empty() || item.trim().is_empty() {
                    continue;
                }
                let index = match state
                    .inventory
                    .iter()
                    .position(|h| same_key(&h.owner, owner))
                {
                    Some(index) => index,
                    None => {
                        state.inventory.push(Holder {
                            owner: owner.trim().to_owned(),
                            items: Vec::new(),
                        });
                        state.inventory.len() - 1
                    }
                };
                let items = &mut state.inventory[index].items;
                if !items.iter().any(|existing| same_key(existing, item)) {
                    items.push(item.trim().to_owned());
                }
            }
            Op::ItemRemove { owner, item } => {
                let Some(index) = state
                    .inventory
                    .iter()
                    .position(|h| same_key(&h.owner, owner))
                else {
                    continue;
                };
                let items = &mut state.inventory[index].items;
                let wanted = normalize(item);
                let found = items
                    .iter()
                    .position(|existing| normalize(existing) == wanted)
                    .or_else(|| {
                        items.iter().position(|existing| {
                            !wanted.is_empty() && normalize(existing).contains(&wanted)
                        })
                    });
                if let Some(position) = found {
                    items.remove(position);
                }
                if items.is_empty() {
                    state.inventory.remove(index);
                }
            }
            Op::Truth {
                id,
                truth,
                hidden_from,
            } => {
                if id.trim().is_empty() || truth.trim().is_empty() {
                    continue;
                }
                if let Some(existing) = truth_mut(&mut state.notebook, id) {
                    truth.trim().clone_into(&mut existing.truth);
                    hidden_from.trim().clone_into(&mut existing.hidden_from);
                } else {
                    state.notebook.push(Truth {
                        id: id.trim().to_owned(),
                        truth: truth.trim().to_owned(),
                        hidden_from: hidden_from.trim().to_owned(),
                        revealed: false,
                        clues: Vec::new(),
                    });
                }
            }
            Op::Clue { id, text, found } => {
                if let Some(existing) = truth_mut(&mut state.notebook, id)
                    && !text.trim().is_empty()
                {
                    existing.clues.push(Clue {
                        text: text.trim().to_owned(),
                        message_id: message_id.to_owned(),
                        found: *found,
                    });
                }
            }
            Op::Reveal { id, revealed } => {
                if let Some(existing) = truth_mut(&mut state.notebook, id) {
                    existing.revealed = *revealed;
                }
            }
            Op::TruthRemove { id } => state.notebook.retain(|t| !same_key(&t.id, id)),
        }
    }
}

/// The state after every delta, in order.
#[must_use]
pub fn fold<'a>(deltas: impl IntoIterator<Item = &'a StateDelta>) -> HelperState {
    let mut state = HelperState::default();
    for delta in deltas {
        apply(&mut state, &delta.ops, &delta.message_id);
    }
    state
}

/// Truths not revealed yet.
#[must_use]
pub fn live_truths(state: &HelperState) -> usize {
    state
        .notebook
        .iter()
        .filter(|truth| !truth.revealed)
        .count()
}

/// Drops new truths beyond `cap` live ones (rewrites of existing ids stay).
#[must_use]
pub fn limit_new_truths(ops: Vec<Op>, state: &HelperState, cap: usize) -> Vec<Op> {
    let mut live = live_truths(state);
    let mut seen: Vec<String> = state.notebook.iter().map(|t| normalize(&t.id)).collect();
    ops.into_iter()
        .filter(|op| {
            let Op::Truth { id, .. } = op else {
                return true;
            };
            let key = normalize(id);
            if seen.contains(&key) {
                return true;
            }
            if live >= cap {
                return false;
            }
            live += 1;
            seen.push(key);
            true
        })
        .collect()
}

fn diff_fields(tracker: Tracker, entity: &str, old: &[Field], new: &[Field], ops: &mut Vec<Op>) {
    for field in old {
        if !new.iter().any(|f| same_key(&f.name, &field.name)) {
            ops.push(Op::Set {
                tracker,
                entity: entity.to_owned(),
                field: field.name.clone(),
                value: String::new(),
            });
        }
    }
    for field in new {
        if field.name.trim().is_empty() {
            continue;
        }
        let before = old.iter().find(|f| same_key(&f.name, &field.name));
        if before.is_none_or(|before| before.value.trim() != field.value.trim()) {
            ops.push(Op::Set {
                tracker,
                entity: entity.to_owned(),
                field: field.name.clone(),
                value: field.value.clone(),
            });
        }
    }
}

/// The operations that turn `old` into `new` (what a manual edit saves).
#[must_use]
pub fn diff(old: &HelperState, new: &HelperState) -> Vec<Op> {
    let mut ops = Vec::new();
    diff_fields(Tracker::Scene, "", &old.scene, &new.scene, &mut ops);
    for tracker in [Tracker::Characters, Tracker::Bonds, Tracker::Npcs] {
        let (Some(before), Some(after)) = (old.entities(tracker), new.entities(tracker)) else {
            continue;
        };
        for entity in before {
            if !after.iter().any(|e| same_key(&e.name, &entity.name)) {
                ops.push(Op::Remove {
                    tracker,
                    entity: entity.name.clone(),
                });
            }
        }
        for entity in after {
            if entity.name.trim().is_empty() {
                continue;
            }
            let previous = before
                .iter()
                .find(|e| same_key(&e.name, &entity.name))
                .map_or(&[][..], |e| e.fields.as_slice());
            diff_fields(tracker, &entity.name, previous, &entity.fields, &mut ops);
        }
    }
    for holder in &old.inventory {
        if !new
            .inventory
            .iter()
            .any(|h| same_key(&h.owner, &holder.owner))
        {
            ops.push(Op::Remove {
                tracker: Tracker::Inventory,
                entity: holder.owner.clone(),
            });
        }
    }
    for holder in &new.inventory {
        if holder.owner.trim().is_empty() {
            continue;
        }
        let previous = old
            .inventory
            .iter()
            .find(|h| same_key(&h.owner, &holder.owner))
            .map_or(&[][..], |h| h.items.as_slice());
        for item in previous {
            if !holder.items.iter().any(|i| same_key(i, item)) {
                ops.push(Op::ItemRemove {
                    owner: holder.owner.clone(),
                    item: item.clone(),
                });
            }
        }
        for item in &holder.items {
            if !item.trim().is_empty() && !previous.iter().any(|i| same_key(i, item)) {
                ops.push(Op::ItemAdd {
                    owner: holder.owner.clone(),
                    item: item.clone(),
                });
            }
        }
    }
    for truth in &old.notebook {
        if !new.notebook.iter().any(|t| same_key(&t.id, &truth.id)) {
            ops.push(Op::TruthRemove {
                id: truth.id.clone(),
            });
        }
    }
    for truth in &new.notebook {
        if truth.id.trim().is_empty() || truth.truth.trim().is_empty() {
            continue;
        }
        let previous = old.notebook.iter().find(|t| same_key(&t.id, &truth.id));
        if previous.is_none_or(|p| {
            p.truth.trim() != truth.truth.trim() || p.hidden_from.trim() != truth.hidden_from.trim()
        }) {
            ops.push(Op::Truth {
                id: truth.id.clone(),
                truth: truth.truth.clone(),
                hidden_from: truth.hidden_from.clone(),
            });
        }
        if previous.map_or(truth.revealed, |p| p.revealed != truth.revealed) {
            ops.push(Op::Reveal {
                id: truth.id.clone(),
                revealed: truth.revealed,
            });
        }
    }
    ops
}
