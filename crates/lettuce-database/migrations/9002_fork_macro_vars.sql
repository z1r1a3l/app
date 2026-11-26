-- Fork-only (z1r1a3l/app): SillyTavern macro variables and dice logs.
-- Ids 9001+ never collide with upstream's sequential migrations.
CREATE TABLE fork_chat_vars (
    conversation_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(name) > 0),
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (conversation_id, name)
) STRICT;

CREATE TABLE fork_global_vars (
    name TEXT PRIMARY KEY CHECK (length(name) > 0),
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- The `{{roll}}` results of one generation, found again by the fingerprint
-- of the prompt that was sent.
CREATE TABLE fork_macro_rolls (
    conversation_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    rolls_json TEXT NOT NULL CHECK (json_valid(rolls_json)),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (conversation_id, fingerprint)
) STRICT;
CREATE INDEX fork_macro_rolls_age_idx ON fork_macro_rolls(conversation_id, created_at);
