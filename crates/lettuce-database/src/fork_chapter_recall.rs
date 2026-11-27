//! Fork-only storage for folded-chapter recall (migration 9006): the global
//! switch and limit, plus a query for inactive chapter summary units.

use lettuce_memory::{ChapterRecallConfig, ChapterRecallSource, FoldedChapter};
use rusqlite::{OptionalExtension, params};

use crate::memory::memory_cycle_revert_adapter::UNDONE;
use crate::{Database, DatabaseError};

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

const CONFIG_COLUMNS: &[&str] = &["id", "enabled", "chapter_limit", "updated_at"];

impl Database {
    pub fn fork_chapter_recall_config(&self) -> Result<ChapterRecallConfig, DatabaseError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT enabled, chapter_limit FROM fork_chapter_recall_config WHERE id = 'global'",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        Ok(row
            .map(|(enabled, limit)| {
                ChapterRecallConfig {
                    enabled: enabled == 1,
                    limit: u32::try_from(limit).unwrap_or(ChapterRecallConfig::default().limit),
                }
                .clamp()
            })
            .unwrap_or_default())
    }

    pub fn fork_chapter_recall_config_set(
        &self,
        config: &ChapterRecallConfig,
    ) -> Result<(), DatabaseError> {
        let config = config.clone().clamp();
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO fork_chapter_recall_config (id, enabled, chapter_limit, updated_at)
             VALUES ('global', ?1, ?2, ?3)
             ON CONFLICT (id) DO UPDATE SET enabled = excluded.enabled,
                 chapter_limit = excluded.chapter_limit, updated_at = excluded.updated_at
             WHERE enabled IS NOT excluded.enabled OR chapter_limit IS NOT excluded.chapter_limit",
            params![
                if config.enabled { 1i64 } else { 0i64 },
                i64::from(config.limit),
                now_millis()
            ],
        )?;
        Ok(())
    }

    /// Inactive `kind=chapter` units folded under a still-valid parent.
    pub fn fork_folded_chapters(
        &self,
        space_id: &str,
        conversation_id: &str,
        branch_id: &str,
    ) -> Result<Vec<FoldedChapter>, DatabaseError> {
        let parent_undone = UNDONE.replace("run.", "parent_run.");
        let sql = format!(
            "SELECT u.text, u.token_count, u.window_start, u.window_end
               FROM memory_summary_units u
               JOIN dynamic_memory_runs run ON run.id = u.created_by_run_id
              WHERE u.space_id = ?1
                AND u.conversation_id = ?2
                AND u.branch_id = ?3
                AND u.kind = 'chapter'
                AND NOT {UNDONE}
                AND EXISTS (
                    SELECT 1 FROM dynamic_memory_summary_checkpoints checkpoint
                     WHERE checkpoint.run_id = run.id
                )
                AND EXISTS (
                    SELECT 1
                      FROM memory_summary_unit_inputs edge
                      JOIN memory_summary_units parent ON parent.id = edge.parent_id
                      JOIN dynamic_memory_runs parent_run ON parent_run.id = parent.created_by_run_id
                     WHERE edge.child_id = u.id
                       AND parent.space_id = u.space_id
                       AND parent.conversation_id = u.conversation_id
                       AND parent.branch_id = u.branch_id
                       AND NOT {parent_undone}
                       AND EXISTS (
                           SELECT 1 FROM dynamic_memory_summary_checkpoints checkpoint
                            WHERE checkpoint.run_id = parent_run.id
                       )
                )
              ORDER BY u.window_start ASC, u.window_end ASC, u.id ASC"
        );
        let connection = self.connection()?;
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query(params![space_id, conversation_id, branch_id])?;
        let mut chapters = Vec::new();
        while let Some(row) = rows.next()? {
            let text: String = row.get(0)?;
            let token_count: i64 = row.get(1)?;
            let window_start: i64 = row.get(2)?;
            let window_end: i64 = row.get(3)?;
            if text.trim().is_empty() {
                continue;
            }
            chapters.push(FoldedChapter {
                text,
                token_count: u32::try_from(token_count).unwrap_or(0),
                window_start: u64::try_from(window_start).unwrap_or(0),
                window_end: u64::try_from(window_end).unwrap_or(0),
            });
        }
        Ok(chapters)
    }
}

/// Process-wide source backed by this database.
#[derive(Debug)]
pub struct DatabaseChapterRecallSource(pub std::sync::Weak<Database>);

impl ChapterRecallSource for DatabaseChapterRecallSource {
    fn config(&self) -> ChapterRecallConfig {
        self.0
            .upgrade()
            .and_then(|database| database.fork_chapter_recall_config().ok())
            .unwrap_or_default()
    }

    fn folded_chapters(
        &self,
        space_id: &str,
        conversation_id: &str,
        branch_id: &str,
    ) -> Vec<FoldedChapter> {
        self.0
            .upgrade()
            .and_then(|database| {
                database
                    .fork_folded_chapters(space_id, conversation_id, branch_id)
                    .ok()
            })
            .unwrap_or_default()
    }
}

pub(crate) fn read_backup_in(
    transaction: &rusqlite::Transaction<'_>,
) -> rusqlite::Result<lettuce_transfer::ForkChapterRecallBackup> {
    use crate::legacy::legacy_import_backup_adapter::read_rows;
    Ok(lettuce_transfer::ForkChapterRecallBackup {
        config: read_rows(
            transaction,
            "fork_chapter_recall_config",
            CONFIG_COLUMNS,
            None,
        )?,
    })
}

pub(crate) fn insert_restored_in(
    transaction: &rusqlite::Transaction<'_>,
    backup: &lettuce_transfer::ForkChapterRecallBackup,
) -> rusqlite::Result<()> {
    use crate::legacy::legacy_import_backup_adapter::insert_rows;
    insert_rows(
        transaction,
        "fork_chapter_recall_config",
        CONFIG_COLUMNS,
        &backup.config,
    )
}

pub(crate) const SYNC_KIND: &str = "fork_chapter_recall_config";
pub(crate) const SYNC_SCHEMA: &str = "fork.chapter_recall_config";
pub(crate) const SYNC_VERSION: u32 = 1;

pub(crate) const SYNC_ROWS: crate::sync::row_sync_adapter::RowTable =
    crate::sync::row_sync_adapter::RowTable {
        table: "fork_chapter_recall_config",
        key: &["id"],
        immutable: false,
        columns: &["enabled", "chapter_limit", "updated_at"],
        revision: false,
        identity: &[],
        references: &[],
        scan_filter: None,
    };

#[cfg(test)]
mod tests {
    use lettuce_memory::{
        ChapterRecallConfig, MemoryOrigin, MemorySummaryUnitKind, MemorySummaryUnitRepository,
        MemorySummaryUnitScope, NewMemorySummaryUnit,
    };
    use lettuce_types::{
        ConversationBranchId, ConversationId, DynamicMemoryAttemptId, DynamicMemoryRunId, JobId,
        MemorySpaceId, MemorySummaryUnitId, TimestampMillis,
    };
    use rusqlite::params;

    use crate::Database;

    #[test]
    fn config_defaults_and_round_trips_clamped() {
        let database = Database::open_in_memory().expect("db");
        assert_eq!(
            database.fork_chapter_recall_config().expect("get"),
            ChapterRecallConfig::default()
        );
        database
            .fork_chapter_recall_config_set(&ChapterRecallConfig {
                enabled: true,
                limit: 9,
            })
            .expect("set");
        let stored = database.fork_chapter_recall_config().expect("get");
        assert!(stored.enabled);
        assert_eq!(stored.limit, 2);
        database
            .fork_chapter_recall_config_set(&ChapterRecallConfig {
                enabled: false,
                limit: 1,
            })
            .expect("set");
        let stored = database.fork_chapter_recall_config().expect("get");
        assert!(!stored.enabled);
        assert_eq!(stored.limit, 1);
    }

    struct Scope {
        database: Database,
        scope: MemorySummaryUnitScope,
    }

    impl Scope {
        fn new() -> Self {
            let database = Database::open_in_memory().expect("database");
            let scope = MemorySummaryUnitScope {
                space_id: MemorySpaceId::new(),
                conversation_id: ConversationId::new(),
                branch_id: ConversationBranchId::new(),
            };
            let mut connection = database.connection().expect("connection");
            let transaction = connection.transaction().expect("transaction");
            transaction
                .execute(
                    "INSERT INTO conversations (id, kind, lifecycle, title, active_branch_id, kind_json, revision, created_at, updated_at)
                     VALUES (?1, 'direct', 'active', 'Chat', ?2, '{\"format_version\":1,\"value\":{\"kind\":\"direct\"}}', 1, 1, 1)",
                    params![scope.conversation_id.to_string(), scope.branch_id.to_string()],
                )
                .expect("conversation");
            transaction
                .execute(
                    "INSERT INTO conversation_branches (conversation_id, id, status, revision, created_at, updated_at)
                     VALUES (?1, ?2, 'active', 1, 1, 1)",
                    params![scope.conversation_id.to_string(), scope.branch_id.to_string()],
                )
                .expect("branch");
            transaction
                .execute(
                    "INSERT INTO memory_spaces (id, revision) VALUES (?1, 1)",
                    [scope.space_id.to_string()],
                )
                .expect("space");
            transaction
                .execute(
                    "INSERT INTO conversation_memory_spaces (conversation_id, branch_id, space_id, pooled)
                     VALUES (?1, ?2, ?3, 0)",
                    params![
                        scope.conversation_id.to_string(),
                        scope.branch_id.to_string(),
                        scope.space_id.to_string(),
                    ],
                )
                .expect("space binding");
            transaction.commit().expect("commit");
            drop(connection);
            Self { database, scope }
        }

        fn insert_run(&self, created_at: i64) -> (DynamicMemoryRunId, DynamicMemoryAttemptId) {
            let run_id = DynamicMemoryRunId::new();
            let attempt_id = DynamicMemoryAttemptId::new();
            let job_id = JobId::new();
            let connection = self.database.connection().expect("connection");
            connection
                .execute(
                    "INSERT INTO dynamic_memory_runs (
                        id, conversation_id, branch_id, space_id, time_awareness_enabled,
                        supersession_enabled, structured_fallback_format, summary_message_interval,
                        summary_window_start, summary_window_end, starting_memory_json,
                        profile_json, tool_request_json, created_at
                    ) VALUES (?1, ?2, ?3, ?4, 0, 0, 'xml', 20, 0, 40,
                              '{\"format_version\":1,\"value\":{}}',
                              '{\"format_version\":1,\"value\":{}}',
                              '{\"format_version\":1,\"value\":{\"definitions\":[]}}', ?5)",
                    params![
                        run_id.to_string(),
                        self.scope.conversation_id.to_string(),
                        self.scope.branch_id.to_string(),
                        self.scope.space_id.to_string(),
                        created_at,
                    ],
                )
                .expect("run");
            connection
                .execute(
                    "INSERT INTO dynamic_memory_run_attempts (
                        job_attempt, run_id, id, ordinal, retry_parent_id, job_id, status,
                        failure, revision, created_at, started_at, finished_at, updated_at
                    ) VALUES (NULL, ?1, ?2, 0, NULL, ?3, 'succeeded', NULL, 1, ?4, ?4, ?4, ?4)",
                    params![
                        run_id.to_string(),
                        attempt_id.to_string(),
                        job_id.to_string(),
                        created_at,
                    ],
                )
                .expect("attempt");
            (run_id, attempt_id)
        }

        fn checkpoint(&self, run_id: DynamicMemoryRunId, attempt_id: DynamicMemoryAttemptId) {
            let connection = self.database.connection().expect("connection");
            connection
                .execute(
                    "INSERT INTO dynamic_memory_summary_checkpoints (
                        run_id, attempt_id, space_id, expected_memory_revision,
                        resulting_memory_revision, summary_text, token_count,
                        request_context_json, settled_at
                    ) VALUES (?1, ?2, ?3, 1, 2, 'checkpoint summary', 10,
                              '{\"format_version\":1,\"value\":{}}', 50)",
                    params![
                        run_id.to_string(),
                        attempt_id.to_string(),
                        self.scope.space_id.to_string(),
                    ],
                )
                .expect("checkpoint");
        }

        fn unit(
            &self,
            run_id: DynamicMemoryRunId,
            kind: MemorySummaryUnitKind,
            level: u32,
            start: u64,
            end: u64,
            text: &str,
        ) -> NewMemorySummaryUnit {
            NewMemorySummaryUnit {
                id: MemorySummaryUnitId::new(),
                scope: self.scope,
                created_by_run_id: run_id,
                kind,
                level,
                origin: MemoryOrigin::Model,
                window_start: start,
                window_end: end,
                time_start: None,
                time_end: None,
                text: text.to_owned(),
                token_count: 25,
                created_at: TimestampMillis::new(100),
            }
        }
    }

    #[test]
    fn folded_chapters_are_inactive_checkpointed_chapters_not_cycles() {
        let fixture = Scope::new();
        let (run, attempt) = fixture.insert_run(10);
        fixture.checkpoint(run, attempt);
        let cycle = fixture
            .database
            .insert_summary_unit(
                fixture.unit(
                    run,
                    MemorySummaryUnitKind::Cycle,
                    0,
                    0,
                    10,
                    "Cycle text about the tavern.",
                ),
                &[],
            )
            .expect("cycle");
        let chapter = fixture
            .database
            .insert_summary_unit(
                fixture.unit(
                    run,
                    MemorySummaryUnitKind::Chapter,
                    1,
                    0,
                    20,
                    "Chapter: Kassian held the tavern.",
                ),
                &[cycle.id],
            )
            .expect("chapter");
        // Active chapter: not folded yet.
        let folded = fixture
            .database
            .fork_folded_chapters(
                &fixture.scope.space_id.to_string(),
                &fixture.scope.conversation_id.to_string(),
                &fixture.scope.branch_id.to_string(),
            )
            .expect("query");
        assert!(folded.is_empty(), "{folded:?}");

        fixture
            .database
            .insert_summary_unit(
                fixture.unit(
                    run,
                    MemorySummaryUnitKind::Chapter,
                    2,
                    0,
                    40,
                    "Arc of the tavern war.",
                ),
                &[chapter.id],
            )
            .expect("arc");

        let folded = fixture
            .database
            .fork_folded_chapters(
                &fixture.scope.space_id.to_string(),
                &fixture.scope.conversation_id.to_string(),
                &fixture.scope.branch_id.to_string(),
            )
            .expect("query");
        assert_eq!(folded.len(), 1, "{folded:?}");
        assert!(folded[0].text.contains("Kassian"), "{folded:?}");
        assert!(!folded.iter().any(|chapter| chapter.text.contains("Cycle")));
        assert!(!folded.iter().any(|chapter| chapter.text.contains("Arc of")));
    }
}
