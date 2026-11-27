//! Fork-only: folded-chapter recall settings.

use serde::{Deserialize, Serialize};

/// Global switch and 1–2 chapter limit. Off by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct ChapterRecallConfigDto {
    pub enabled: bool,
    pub limit: u32,
}

impl Default for ChapterRecallConfigDto {
    fn default() -> Self {
        Self {
            enabled: false,
            limit: 2,
        }
    }
}
