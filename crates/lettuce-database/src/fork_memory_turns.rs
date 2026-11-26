//! Fork-only storage for user-set memory turn ranges (migration 9003).
//!
//! Model-created memories show a `cycle_label` like "10-20", derived from the
//! cycle that wrote them. Manual create/edit has no cycle, so the range is
//! stored here and read back as the same label.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};

use crate::{ApiOperationTransaction, Database, DatabaseError};

/// A 1-based inclusive message range, shown as `"{start}-{end}"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryTurnRange {
    pub start: u64,
    pub end: u64,
}

impl MemoryTurnRange {
    #[must_use]
    pub fn new(start: u64, end: u64) -> Option<Self> {
        (start >= 1 && end >= 1 && start <= end).then_some(Self { start, end })
    }

    #[must_use]
    pub fn label(self) -> String {
        format!("{}-{}", self.start, self.end)
    }
}

fn range_from_row(start: i64, end: i64) -> Result<MemoryTurnRange, DatabaseError> {
    let start = as_u64(start)?;
    let end = as_u64(end)?;
    MemoryTurnRange::new(start, end).ok_or_else(|| {
        DatabaseError::from(rusqlite::Error::ToSqlConversionFailure(
            "fork_memory_turns row failed validation".into(),
        ))
    })
}

fn as_u64(value: i64) -> Result<u64, DatabaseError> {
    u64::try_from(value).map_err(|error| {
        DatabaseError::from(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
    })
}

fn as_i64(value: u64) -> Result<i64, DatabaseError> {
    i64::try_from(value).map_err(|error| {
        DatabaseError::from(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
    })
}

fn set_in(
    transaction: &rusqlite::Transaction<'_>,
    memory_id: &str,
    space_id: &str,
    range: MemoryTurnRange,
    updated_at: i64,
) -> Result<(), DatabaseError> {
    transaction.execute(
        "INSERT INTO fork_memory_turns (memory_id,space_id,turn_start,turn_end,updated_at) \
             VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(memory_id) DO UPDATE SET \
             space_id=excluded.space_id, turn_start=excluded.turn_start, \
             turn_end=excluded.turn_end, updated_at=excluded.updated_at",
        params![
            memory_id,
            space_id,
            as_i64(range.start)?,
            as_i64(range.end)?,
            updated_at
        ],
    )?;
    Ok(())
}

fn clear_in(transaction: &rusqlite::Transaction<'_>, memory_id: &str) -> Result<(), DatabaseError> {
    transaction.execute(
        "DELETE FROM fork_memory_turns WHERE memory_id=?1",
        params![memory_id],
    )?;
    Ok(())
}

impl Database {
    pub fn fork_memory_turns_get(
        &self,
        memory_id: &str,
    ) -> Result<Option<MemoryTurnRange>, DatabaseError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT turn_start, turn_end FROM fork_memory_turns WHERE memory_id=?1",
                params![memory_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
            .map(|(start, end)| range_from_row(start, end))
            .transpose()
    }

    pub fn fork_memory_turns_for_space(
        &self,
        space_id: &str,
    ) -> Result<HashMap<String, MemoryTurnRange>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT memory_id, turn_start, turn_end FROM fork_memory_turns WHERE space_id=?1",
        )?;
        let rows = statement.query_map(params![space_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut ranges = HashMap::new();
        for row in rows {
            let (memory_id, start, end) = row?;
            ranges.insert(memory_id, range_from_row(start, end)?);
        }
        Ok(ranges)
    }

    /// Ranges keyed by memory id. Sync can place the item in a different
    /// space than the one stored on the row, so reads should not filter by
    /// `space_id`.
    pub fn fork_memory_turns_for_ids(
        &self,
        memory_ids: &[String],
    ) -> Result<HashMap<String, MemoryTurnRange>, DatabaseError> {
        if memory_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT turn_start, turn_end FROM fork_memory_turns WHERE memory_id=?1")?;
        let mut ranges = HashMap::new();
        for memory_id in memory_ids {
            let found = statement
                .query_row(params![memory_id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })
                .optional()?;
            if let Some((start, end)) = found {
                ranges.insert(memory_id.clone(), range_from_row(start, end)?);
            }
        }
        Ok(ranges)
    }
}

impl ApiOperationTransaction<'_, '_> {
    pub fn fork_memory_turns_set(
        &self,
        memory_id: &str,
        space_id: &str,
        range: MemoryTurnRange,
        updated_at: i64,
    ) -> Result<(), DatabaseError> {
        set_in(self.transaction, memory_id, space_id, range, updated_at)
    }

    pub fn fork_memory_turns_clear(&self, memory_id: &str) -> Result<(), DatabaseError> {
        clear_in(self.transaction, memory_id)
    }
}

const TURN_COLUMNS: &[&str] = &[
    "memory_id",
    "space_id",
    "turn_start",
    "turn_end",
    "updated_at",
];

/// The turn-range table as backup rows.
pub(crate) fn read_backup_in(
    transaction: &rusqlite::Transaction<'_>,
) -> rusqlite::Result<lettuce_transfer::ForkMemoryTurnsBackup> {
    use crate::legacy::legacy_import_backup_adapter::read_rows;
    Ok(lettuce_transfer::ForkMemoryTurnsBackup {
        turns: read_rows(transaction, "fork_memory_turns", TURN_COLUMNS, None)?,
    })
}

/// Writes restored turn-range rows into the (empty) restore target.
pub(crate) fn insert_restored_in(
    transaction: &rusqlite::Transaction<'_>,
    backup: &lettuce_transfer::ForkMemoryTurnsBackup,
) -> rusqlite::Result<()> {
    use crate::legacy::legacy_import_backup_adapter::insert_rows;
    insert_rows(
        transaction,
        "fork_memory_turns",
        TURN_COLUMNS,
        &backup.turns,
    )
}

/// Sync kind for the turn-range table (fork-only; both devices run the fork).
pub(crate) const SYNC_KIND: &str = "fork_memory_turns";
pub(crate) const SYNC_SCHEMA: &str = "fork.memory_turns";
pub(crate) const SYNC_VERSION: u32 = 1;

/// Turn ranges sync row by row, keyed by memory id.
pub(crate) const SYNC_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_memory_turns",
        key: &["memory_id"],
        immutable: false,
        columns: &["space_id", "turn_start", "turn_end", "updated_at"],
        revision: false,
        identity: &[],
        references: &[],
        scan_filter: None,
    };

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    fn open() -> Database {
        Database::open_in_memory().expect("database")
    }

    #[test]
    fn new_rejects_zero_and_reversed_ranges() {
        assert!(MemoryTurnRange::new(0, 10).is_none());
        assert!(MemoryTurnRange::new(1, 0).is_none());
        assert!(MemoryTurnRange::new(20, 10).is_none());
        assert_eq!(
            MemoryTurnRange::new(10, 20).expect("range"),
            MemoryTurnRange { start: 10, end: 20 }
        );
        assert_eq!(MemoryTurnRange::new(7, 7).expect("single").label(), "7-7");
    }

    #[test]
    fn set_get_replace_and_clear_round_trip() {
        let database = open();
        let range = MemoryTurnRange::new(10, 20).expect("range");
        {
            let mut connection = database.connection().expect("lock");
            let tx = connection.transaction().expect("tx");
            set_in(&tx, "mem-1", "space-1", range, 1).expect("set");
            tx.commit().expect("commit");
        }

        assert_eq!(
            database.fork_memory_turns_get("mem-1").expect("get"),
            Some(range)
        );
        let for_space = database
            .fork_memory_turns_for_space("space-1")
            .expect("space");
        assert_eq!(for_space.get("mem-1").copied(), Some(range));
        assert!(
            database
                .fork_memory_turns_for_space("space-other")
                .expect("other")
                .is_empty()
        );

        let updated = MemoryTurnRange::new(15, 15).expect("single");
        {
            let mut connection = database.connection().expect("lock");
            let tx = connection.transaction().expect("tx");
            set_in(&tx, "mem-1", "space-1", updated, 2).expect("replace");
            tx.commit().expect("commit");
        }
        assert_eq!(
            database.fork_memory_turns_get("mem-1").expect("get"),
            Some(updated)
        );

        {
            let mut connection = database.connection().expect("lock");
            let tx = connection.transaction().expect("tx");
            clear_in(&tx, "mem-1").expect("clear");
            tx.commit().expect("commit");
        }
        assert_eq!(database.fork_memory_turns_get("mem-1").expect("get"), None);
    }

    #[test]
    fn table_checks_reject_invalid_rows() {
        let database = open();
        let connection = database.connection().expect("lock");
        for sql in [
            "INSERT INTO fork_memory_turns (memory_id,space_id,turn_start,turn_end,updated_at) VALUES ('m','s',0,2,1)",
            "INSERT INTO fork_memory_turns (memory_id,space_id,turn_start,turn_end,updated_at) VALUES ('m','s',5,4,1)",
            "INSERT INTO fork_memory_turns (memory_id,space_id,turn_start,turn_end,updated_at) VALUES ('','s',1,2,1)",
        ] {
            assert!(connection.execute(sql, []).is_err(), "{sql}");
        }
    }

    fn store(
        database: &Database,
        memory_id: &str,
        space_id: &str,
        range: MemoryTurnRange,
        at: i64,
    ) {
        let mut connection = database.connection().expect("lock");
        let tx = connection.transaction().expect("tx");
        set_in(&tx, memory_id, space_id, range, at).expect("set");
        tx.commit().expect("commit");
    }

    #[test]
    fn turn_ranges_survive_a_backup_round_trip() {
        let database = open();
        let first = MemoryTurnRange::new(10, 20).expect("range");
        let second = MemoryTurnRange::new(7, 7).expect("single");
        store(&database, "mem-1", "space-1", first, 1);
        store(&database, "mem-2", "space-2", second, 2);
        let (graph, restored) =
            crate::backup::restore_writer::tests::backup_round_trip(&database, |_| {});
        assert_eq!(graph.fork_memory_turns.turns.len(), 2);
        assert_eq!(
            restored.fork_memory_turns_get("mem-1").expect("get"),
            Some(first)
        );
        assert_eq!(
            restored.fork_memory_turns_get("mem-2").expect("get"),
            Some(second)
        );
        let json = serde_json::to_value(&graph).expect("graph json");
        assert!(json.get("fork_memory_turns").is_some());
    }

    #[test]
    fn a_backup_without_turn_ranges_leaves_the_field_out() {
        let database = open();
        let graph = crate::backup::restore_writer::tests::assert_backup_round_trip(&database);
        let json = serde_json::to_value(&graph).expect("graph json");
        assert!(
            json.get("fork_memory_turns").is_none(),
            "upstream builds can still read it"
        );
    }

    fn sync_all(from: &Database, to: &Database, at: i64) {
        use lettuce_sync::{IncomingBatchState, IncomingChangeRepository, LocalChangeJournal};
        use lettuce_types::TimestampMillis;
        from.journal_current_state(TimestampMillis::new(at))
            .expect("scan source");
        to.journal_current_state(TimestampMillis::new(at))
            .expect("scan target");
        loop {
            let batch = from
                .outbound_changes(
                    &to.local_frontier().expect("frontier"),
                    lettuce_sync::MAX_OUTBOUND_CHANGES,
                    lettuce_sync::MAX_OUTBOUND_PAYLOAD_BYTES,
                )
                .expect("outbound");
            if batch.changes.is_empty() {
                return;
            }
            let id = lettuce_types::OperationId::new();
            to.stage_incoming_batch(
                lettuce_sync::SyncDeviceId::new(),
                id,
                &lettuce_sync::canonical_batch_hash(&batch.changes),
                &batch.changes,
                TimestampMillis::new(at),
            )
            .expect("stage");
            assert_eq!(
                to.apply_incoming_batch(id, TimestampMillis::new(at))
                    .expect("apply")
                    .state,
                IncomingBatchState::Committed
            );
        }
    }

    fn journaled_kinds(database: &Database) -> Vec<String> {
        let connection = database.connection().expect("lock");
        let mut statement = connection
            .prepare("SELECT DISTINCT entity_kind FROM sync_changes ORDER BY 1")
            .expect("stmt");
        statement
            .query_map([], |row| row.get(0))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("kinds")
    }

    #[test]
    fn turn_ranges_sync_to_another_database() {
        use lettuce_sync::LocalChangeJournal;
        use lettuce_types::TimestampMillis;

        let host = open();
        let client = open();
        let first = MemoryTurnRange::new(10, 20).expect("range");
        store(&host, "mem-1", "space-1", first, 1);
        host.journal_current_state(TimestampMillis::new(1))
            .expect("journal");
        let kinds = journaled_kinds(&host);
        assert!(
            kinds.iter().any(|kind| kind == "fork_memory_turns"),
            "journaled kinds: {kinds:?}"
        );

        sync_all(&host, &client, 2);
        assert_eq!(
            client.fork_memory_turns_get("mem-1").expect("get"),
            Some(first)
        );

        let updated = MemoryTurnRange::new(7, 7).expect("single");
        store(&client, "mem-1", "space-1", updated, 3);
        sync_all(&client, &host, 4);
        assert_eq!(
            host.fork_memory_turns_get("mem-1").expect("get"),
            Some(updated)
        );

        {
            let mut connection = client.connection().expect("lock");
            let tx = connection.transaction().expect("tx");
            clear_in(&tx, "mem-1").expect("clear");
            tx.commit().expect("commit");
        }
        sync_all(&client, &host, 5);
        assert_eq!(host.fork_memory_turns_get("mem-1").expect("get"), None);
    }
}
