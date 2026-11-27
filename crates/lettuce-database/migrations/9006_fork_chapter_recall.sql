-- Fork-only (z1r1a3l/app): optional recall of folded chapter summaries.
-- Ids 9001+ never collide with upstream's sequential migrations.
-- Off by default; at most two chapters. The token cap is not a setting.
CREATE TABLE fork_chapter_recall_config (
    id TEXT PRIMARY KEY CHECK (id = 'global'),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    chapter_limit INTEGER NOT NULL CHECK (chapter_limit >= 1 AND chapter_limit <= 2),
    updated_at INTEGER NOT NULL
) STRICT;
