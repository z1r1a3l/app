-- Fork-only (z1r1a3l/app): SillyTavern-style regex scripts.
-- Ids 9001+ never collide with upstream's sequential migrations.
CREATE TABLE fork_regex_scripts (
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global', 'preset', 'character')),
    scope_id TEXT NOT NULL DEFAULT '',
    id TEXT NOT NULL CHECK (length(trim(id)) > 0),
    position INTEGER NOT NULL CHECK (position >= 0),
    script_json TEXT NOT NULL CHECK (json_valid(script_json)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (scope_kind, scope_id, id)
) STRICT;
CREATE INDEX fork_regex_scripts_order_idx ON fork_regex_scripts(scope_kind, scope_id, position);

-- Per scope: whether its scripts run, and whether a character card's
-- embedded scripts still wait for the user's decision.
CREATE TABLE fork_regex_scopes (
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global', 'preset', 'character')),
    scope_id TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    pending INTEGER NOT NULL CHECK (pending IN (0, 1)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (scope_kind, scope_id)
) STRICT;
