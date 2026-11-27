//! Fork-only (z1r1a3l/app): helper model settings, switches and tracker
//! changes (migration 9004) as rows of the provider graph.

use serde::{Deserialize, Serialize};

use crate::BackupSqlRow;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkHelperBackup {
    #[serde(default)]
    pub config: Vec<BackupSqlRow>,
    #[serde(default)]
    pub scopes: Vec<BackupSqlRow>,
    #[serde(default)]
    pub deltas: Vec<BackupSqlRow>,
}

impl ForkHelperBackup {
    /// Nothing to keep; the graph then leaves the field out, so a backup
    /// without helper data stays readable by upstream builds.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.config.is_empty() && self.scopes.is_empty() && self.deltas.is_empty()
    }
}
