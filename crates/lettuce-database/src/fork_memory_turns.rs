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
}
