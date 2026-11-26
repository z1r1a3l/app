//! Fork-only (z1r1a3l/app): user-set memory turn ranges (migration 9003)
//! as rows of the provider graph.

use serde::{Deserialize, Serialize};

use crate::BackupSqlRow;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkMemoryTurnsBackup {
    #[serde(default)]
    pub turns: Vec<BackupSqlRow>,
}

impl ForkMemoryTurnsBackup {
    /// Nothing to keep; the graph then leaves the field out, so a backup
    /// without turn ranges stays readable by upstream builds.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }
}
