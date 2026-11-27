//! Fork-only (z1r1a3l/app): the helper model's trackers.
//!
//! After each main reply a separate, cheap model reads the exchange and
//! writes only what changed (`[Characters] Aqua · Outfit: blue dress -> bath
//! towel`). The app keeps those change lines per reply variant, folds the
//! ones on the shown path into the full state, and puts one compact
//! "Current state" block into the main prompt. Swiping, editing or deleting
//! a reply drops its changes because the fold only reads shown variants.
//!
//! Pure: no storage or network here. `store` is the slot the database fills.

mod clue;
mod delta;
mod locks;
mod model;
mod prompt;
mod render;
mod store;

#[cfg(test)]
mod tests;

pub use clue::clue_found;
pub use delta::{ParsedDelta, parse_delta};
pub use locks::{BondLock, apply_bond_locks, drop_locked_bond_ops, is_locked, touches_locked_bond};
pub use model::{
    Clue, Entity, Field, HelperState, Holder, Op, StateDelta, Tracker, Trackers, Truth, apply,
    diff, fold, limit_new_truths, live_truths, normalize, same_key,
};
pub use prompt::{
    DEFAULT_PROMPT, HelperInput, HelperLine, MAX_LIVE_TRUTHS, helper_system_prompt,
    helper_user_message,
};
pub use render::{render_state_block, render_state_block_with_locks};
pub use store::{
    DeltaSource, HelperStateSource, StoredDelta, clear_state_source, set_state_source,
    state_for_path, state_source,
};
