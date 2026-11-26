//! Fork-only: SillyTavern-compatible macros with chat and global variables.
//!
//! Supported: `{{// comment}}`, `{{trim}}`, `{{noop}}`, `{{newline}}`,
//! `{{space}}`, `{{user}}`/`{{char}}` (when the host passes names),
//! `{{setvar|getvar|addvar|incvar|decvar|hasvar|deletevar::name::value}}`
//! and their `globalvar` twins, `{{roll::NdM+K}}`, `{{random::a::b}}`,
//! `{{random:a,b}}`, `{{pick::a::b}}`, `{{reverse::text}}`, `{{input}}`,
//! `{{lastMessage}}`, `{{lastUserMessage}}`, `{{lastCharMessage}}`,
//! `{{if cond}}…{{else}}…{{/if}}` (and `{{if cond::text}}`), scoped
//! `{{setvar::x}}…{{/setvar}}` / `{{trim}}…{{/trim}}` / `{{//}}…{{///}}`,
//! the `{{name arg}}` space form, `{{.var}}` / `{{$var}}` with their
//! operators, and `getvarkey` / `setvarkey` (plus global twins).
//! Everything else (including the app's own `{{char.desc}}`-style
//! placeholders and `{{#if name}}` blocks on names only the app knows) is
//! left as written.
//!
//! Kept in its own module so upstream rebases touch as little as possible.

mod engine;
mod number;
mod store;
#[cfg(test)]
mod tests;

pub use engine::{
    DiceRoll, MAX_VALUE_BYTES, MacroEnv, MacroRun, VarChanges, Vars, context_fingerprint,
};
pub use store::{MacroVarStore, clear_var_store, set_var_store, var_store};
