//! User-locked bonds: the helper cannot rewrite them.
//!
//! Locks live outside reply deltas, so a swipe does not clear them. After
//! folding a path, `apply_bond_locks` writes the frozen fields back. Helper
//! ops that touch a locked name are dropped before they are stored.

use serde::{Deserialize, Serialize};

use super::model::{Entity, Field, HelperState, Op, Tracker, same_key};

/// A bond the user froze: name and fields as they must stay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BondLock {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<Field>,
}

impl BondLock {
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        same_key(&self.name, name)
    }
}

/// Whether `name` is in `locks`.
#[must_use]
pub fn is_locked(name: &str, locks: &[BondLock]) -> bool {
    locks.iter().any(|lock| lock.matches(name))
}

/// True when a helper op would change or remove a locked bond.
#[must_use]
pub fn touches_locked_bond(op: &Op, locks: &[BondLock]) -> bool {
    match op {
        Op::Set {
            tracker: Tracker::Bonds,
            entity,
            ..
        }
        | Op::Remove {
            tracker: Tracker::Bonds,
            entity,
        } => is_locked(entity, locks),
        _ => false,
    }
}

/// Drops helper ops that would rewrite a locked bond.
#[must_use]
pub fn drop_locked_bond_ops(ops: &[Op], locks: &[BondLock]) -> Vec<Op> {
    ops.iter()
        .filter(|op| !touches_locked_bond(op, locks))
        .cloned()
        .collect()
}

/// Writes each lock over the folded state: restores fields, and puts a
/// missing bond back if a helper remove had dropped it.
pub fn apply_bond_locks(state: &mut HelperState, locks: &[BondLock]) {
    for lock in locks {
        let name = lock.name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(entity) = state
            .bonds
            .iter_mut()
            .find(|entity| same_key(&entity.name, name))
        {
            entity.name = lock.name.clone();
            entity.fields = lock.fields.clone();
        } else {
            state.bonds.push(Entity {
                name: lock.name.clone(),
                fields: lock.fields.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fork_helper::model::{Field, apply};

    fn lock(name: &str, attitude: &str) -> BondLock {
        BondLock {
            name: name.to_owned(),
            fields: vec![Field {
                name: "Attitude".into(),
                value: attitude.into(),
            }],
        }
    }

    fn set_bond(name: &str, value: &str) -> Op {
        Op::Set {
            tracker: Tracker::Bonds,
            entity: name.into(),
            field: "Attitude".into(),
            value: value.into(),
        }
    }

    #[test]
    fn overlay_restores_fields_and_survives_a_remove() {
        let mut state = HelperState::default();
        apply(
            &mut state,
            &[
                set_bond("Кассиан", "враг"),
                set_bond("Трактирщик", "хмурый"),
            ],
            "",
        );
        let locks = vec![lock("кассиан", "враг")];
        apply(
            &mut state,
            &[
                set_bond("Кассиан", "друг"),
                Op::Remove {
                    tracker: Tracker::Bonds,
                    entity: "Кассиан".into(),
                },
            ],
            "",
        );
        assert!(state.bonds.iter().all(|b| !same_key(&b.name, "Кассиан")));
        apply_bond_locks(&mut state, &locks);
        let cassian = state
            .bonds
            .iter()
            .find(|b| same_key(&b.name, "Кассиан"))
            .expect("restored");
        assert_eq!(cassian.fields[0].value, "враг");
        assert_eq!(
            state
                .bonds
                .iter()
                .find(|b| same_key(&b.name, "Трактирщик"))
                .map(|b| b.fields[0].value.as_str()),
            Some("хмурый")
        );
    }

    #[test]
    fn helper_ops_on_a_locked_bond_are_dropped() {
        let locks = vec![lock("Кассиан → Хантер", "враг")];
        let ops = vec![
            set_bond("Кассиан → Хантер", "друг"),
            Op::Remove {
                tracker: Tracker::Bonds,
                entity: "кассиан → хантер".into(),
            },
            set_bond("Трактирщик", "добрее"),
            Op::Set {
                tracker: Tracker::Characters,
                entity: "Кассиан".into(),
                field: "Outfit".into(),
                value: "плащ".into(),
            },
        ];
        let kept = drop_locked_bond_ops(&ops, &locks);
        assert_eq!(kept.len(), 2);
        assert!(!kept.iter().any(|op| touches_locked_bond(op, &locks)));
    }
}
