-- Fork-only (z1r1a3l/app): user-set message turn ranges on memories.
-- Ids 9001+ never collide with upstream's sequential migrations.
-- Shown as cycle_label ("10-20"), the same field model-created memories use.
CREATE TABLE fork_memory_turns (
    memory_id TEXT NOT NULL PRIMARY KEY CHECK (length(trim(memory_id)) > 0),
    space_id TEXT NOT NULL CHECK (length(trim(space_id)) > 0),
    turn_start INTEGER NOT NULL CHECK (turn_start >= 1),
    turn_end INTEGER NOT NULL CHECK (turn_end >= 1 AND turn_end >= turn_start),
    updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX fork_memory_turns_space_idx ON fork_memory_turns(space_id);
