-- Fork-only (z1r1a3l/app): per-chat regex scripts and macros.
-- Ids 9001+ never collide with upstream's sequential migrations.
-- Regex scopes gain the 'chat' kind (scope_id = conversation id); SQLite
-- cannot change a CHECK in place, so both tables are rebuilt. Nothing
-- references them, so the copy is plain.
CREATE TABLE fork_regex_scripts_9007 (
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global', 'preset', 'character', 'chat')),
    scope_id TEXT NOT NULL DEFAULT '',
    id TEXT NOT NULL CHECK (length(trim(id)) > 0),
    position INTEGER NOT NULL CHECK (position >= 0),
    script_json TEXT NOT NULL CHECK (json_valid(script_json)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (scope_kind, scope_id, id)
) STRICT;
INSERT INTO fork_regex_scripts_9007 (scope_kind, scope_id, id, position, script_json, updated_at)
    SELECT scope_kind, scope_id, id, position, script_json, updated_at FROM fork_regex_scripts;
DROP TABLE fork_regex_scripts;
ALTER TABLE fork_regex_scripts_9007 RENAME TO fork_regex_scripts;
CREATE INDEX fork_regex_scripts_order_idx ON fork_regex_scripts(scope_kind, scope_id, position);

CREATE TABLE fork_regex_scopes_9007 (
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global', 'preset', 'character', 'chat')),
    scope_id TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    pending INTEGER NOT NULL CHECK (pending IN (0, 1)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (scope_kind, scope_id)
) STRICT;
INSERT INTO fork_regex_scopes_9007 (scope_kind, scope_id, enabled, pending, updated_at)
    SELECT scope_kind, scope_id, enabled, pending, updated_at FROM fork_regex_scopes;
DROP TABLE fork_regex_scopes;
ALTER TABLE fork_regex_scopes_9007 RENAME TO fork_regex_scopes;

-- One row per chat that changed something: skip the global or the
-- characters' scripts here, and the chat's own `{{key}}` macros
-- (a JSON array of {"key","value"}).
CREATE TABLE fork_chat_overrides (
    conversation_id TEXT PRIMARY KEY CHECK (length(conversation_id) > 0),
    skip_global INTEGER NOT NULL CHECK (skip_global IN (0, 1)),
    skip_character INTEGER NOT NULL CHECK (skip_character IN (0, 1)),
    macros_json TEXT NOT NULL CHECK (json_valid(macros_json)),
    updated_at INTEGER NOT NULL
) STRICT;
