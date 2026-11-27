//! Fork-only (z1r1a3l/app): folded-chapter recall settings (migration 9006)
//! as rows of the provider graph.

use serde::{Deserialize, Serialize};

use crate::BackupSqlRow;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkChapterRecallBackup {
    #[serde(default)]
    pub config: Vec<BackupSqlRow>,
}

impl ForkChapterRecallBackup {
    /// Nothing to keep; the graph then leaves the field out, so a backup
    /// without this setting stays readable by upstream builds.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.config.is_empty()
    }
}
