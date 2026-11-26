//! Fork-only (z1r1a3l/app): SillyTavern macro variables and dice logs
//! (migration 9002) as rows of the provider graph.

use serde::{Deserialize, Serialize};

use crate::BackupSqlRow;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkMacrosBackup {
    #[serde(default)]
    pub chat_vars: Vec<BackupSqlRow>,
    #[serde(default)]
    pub global_vars: Vec<BackupSqlRow>,
    #[serde(default)]
    pub rolls: Vec<BackupSqlRow>,
}

impl ForkMacrosBackup {
    /// Nothing to keep; the graph then leaves the field out, so a backup
    /// without variables stays readable by upstream builds.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chat_vars.is_empty() && self.global_vars.is_empty() && self.rolls.is_empty()
    }
}
