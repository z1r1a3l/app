-- Fork-only (z1r1a3l/app): the helper model's settings, switches and the
-- tracker changes each reply variant made. Ids 9001+ never collide with
-- upstream's sequential migrations.
CREATE TABLE fork_helper_config (
    id TEXT NOT NULL PRIMARY KEY CHECK (id = 'global'),
    model_profile_id TEXT,
    prompt TEXT,
    history INTEGER NOT NULL CHECK (history BETWEEN 0 AND 40),
    updated_at INTEGER NOT NULL
) STRICT;

-- Off unless a chat or a character has a row with enabled = 1; a chat's
-- row wins over its character's.
CREATE TABLE fork_helper_scopes (
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('character', 'conversation')),
    scope_id TEXT NOT NULL CHECK (length(trim(scope_id)) > 0),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    trackers_json TEXT NOT NULL CHECK (json_valid(trackers_json)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (scope_kind, scope_id)
) STRICT;

-- One helper read per reply variant (`c:<candidate>` / `r:<revision>`), and
-- manual edits anchored to the reply shown when they were made. Empty
-- message ids are chat-wide.
CREATE TABLE fork_helper_deltas (
    id TEXT NOT NULL PRIMARY KEY CHECK (length(trim(id)) > 0),
    conversation_id TEXT NOT NULL CHECK (length(trim(conversation_id)) > 0),
    message_id TEXT NOT NULL,
    variant_key TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('helper', 'manual')),
    ops_json TEXT NOT NULL CHECK (json_valid(ops_json)),
    raw TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX fork_helper_deltas_chat_idx ON fork_helper_deltas(conversation_id, message_id);
