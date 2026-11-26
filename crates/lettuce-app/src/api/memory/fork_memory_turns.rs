//! Fork hooks: persist and validate user-set memory turn ranges.

use lettuce_contracts::{self as dto, ApiError, ApiErrorCode};
use lettuce_database::{DatabaseError, ForkMemoryTurnRange};
use lettuce_memory::MemoryManualHistory;
use lettuce_types::TimestampMillis;

use super::{Edit, EditFailure};
use crate::api::error::{api_error, invalid_field};

pub enum Action {
    Keep,
    Set {
        memory_id: Option<String>,
        range: ForkMemoryTurnRange,
    },
    Clear {
        memory_id: Option<String>,
    },
}

pub fn action(edit: &Edit) -> Result<Action, ApiError> {
    match edit {
        Edit::Add(request) => match request.turns.as_ref() {
            None => Ok(Action::Keep),
            Some(range) => Ok(Action::Set {
                memory_id: None,
                range: validated(range)?,
            }),
        },
        Edit::Update(request) => match &request.turns {
            dto::MemoryTurnsChange::Keep => Ok(Action::Keep),
            dto::MemoryTurnsChange::Set(None) => Ok(Action::Clear {
                memory_id: Some(request.memory_id.clone()),
            }),
            dto::MemoryTurnsChange::Set(Some(range)) => Ok(Action::Set {
                memory_id: Some(request.memory_id.clone()),
                range: validated(range)?,
            }),
        },
        Edit::Delete(request) => Ok(Action::Clear {
            memory_id: Some(request.memory_id.clone()),
        }),
        _ => Ok(Action::Keep),
    }
}

fn validated(range: &dto::MemoryTurnRange) -> Result<ForkMemoryTurnRange, ApiError> {
    ForkMemoryTurnRange::new(range.start, range.end).ok_or_else(|| {
        invalid_field(
            "turns",
            "turn start and end must be positive, and start must not be after end",
        )
    })
}

pub fn apply(
    scope: &lettuce_database::ApiOperationTransaction<'_, '_>,
    space_id: &str,
    history: &MemoryManualHistory,
    action: &Action,
    now: TimestampMillis,
) -> Result<(), EditFailure> {
    match action {
        Action::Keep => Ok(()),
        Action::Set { memory_id, range } => {
            let memory_id = memory_id_from(memory_id.as_deref(), history, true)?;
            scope
                .fork_memory_turns_set(&memory_id, space_id, *range, now.get())
                .map_err(storage)
        }
        Action::Clear { memory_id } => {
            let memory_id = memory_id_from(memory_id.as_deref(), history, false)?;
            scope.fork_memory_turns_clear(&memory_id).map_err(storage)
        }
    }
}

fn memory_id_from(
    explicit: Option<&str>,
    history: &MemoryManualHistory,
    after: bool,
) -> Result<String, EditFailure> {
    if let Some(id) = explicit.filter(|id| !id.trim().is_empty()) {
        return Ok(id.to_owned());
    }
    let item = if after {
        history.after_item.as_ref().or(history.before_item.as_ref())
    } else {
        history.before_item.as_ref().or(history.after_item.as_ref())
    };
    item.map(|item| item.id.to_string()).ok_or_else(|| {
        EditFailure(api_error(
            ApiErrorCode::Internal,
            "memory turn range is missing a memory id",
        ))
    })
}

fn storage(error: DatabaseError) -> EditFailure {
    EditFailure(api_error(ApiErrorCode::Internal, error.to_string()))
}
