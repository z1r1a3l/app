//! Fork-only (z1r1a3l/app): regex scripts and their scope switches
//! (migration 9001) as rows of the provider graph.

use serde::{Deserialize, Serialize};

use crate::BackupSqlRow;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkRegexBackup {
    #[serde(default)]
    pub scripts: Vec<BackupSqlRow>,
    #[serde(default)]
    pub scopes: Vec<BackupSqlRow>,
    /// Per-chat switches and macros (migration 9007).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat_overrides: Vec<BackupSqlRow>,
}

impl ForkRegexBackup {
    /// Nothing to keep; the graph then leaves the field out, so a backup
    /// without regex scripts stays readable by upstream builds.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty() && self.scopes.is_empty() && self.chat_overrides.is_empty()
    }
}
