-- Fork-only (z1r1a3l/app): user-locked helper bonds. Ids 9001+ never
-- collide with upstream's sequential migrations. Locks live outside
-- helper deltas, so a swipe does not clear them.
CREATE TABLE fork_helper_locks (
    conversation_id TEXT NOT NULL CHECK (length(trim(conversation_id)) > 0),
    bond_key TEXT NOT NULL CHECK (length(trim(bond_key)) > 0),
    bond_name TEXT NOT NULL CHECK (length(trim(bond_name)) > 0),
    fields_json TEXT NOT NULL CHECK (json_valid(fields_json)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (conversation_id, bond_key)
) STRICT;
CREATE INDEX fork_helper_locks_chat_idx ON fork_helper_locks(conversation_id);
