use super::PostgresPruneExecutionResult;
use super::ipc::{IpcEventJournal, IpcEventKind};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::{
    DaemonOperationStatus, DaemonOperationTransitionOptions, StateStore,
};

/// Publishes completion only after Engine deletion and state retirement succeed.
pub(crate) fn publish_postgres_prune_result<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    result: PostgresPruneExecutionResult,
    now_unix_seconds: i64,
) -> Result<(), String>
where
    Store: StateStore,
{
    let (operation, outcome) = result.into_parts();
    let (kind, status) = match outcome {
        Ok(()) => (IpcEventKind::Completed, DaemonOperationStatus::Completed),
        Err(message) => (
            IpcEventKind::Failed {
                code: "postgres_prune_failed".to_owned(),
                message,
            },
            DaemonOperationStatus::Failed,
        ),
    };
    let kind_json = serde_json::to_string(&kind)
        .map_err(|error| format!("failed to encode PostgreSQL prune event: {error}"))?;
    let event = control_plane
        .transition_daemon_operation(DaemonOperationTransitionOptions {
            operation_id: operation.operation_id(),
            expected: DaemonOperationStatus::Running,
            next: status,
            updated_at_unix_seconds: now_unix_seconds,
            event_kind_json: Some(&kind_json),
            event_retention_limit: event_journal.capacity(),
        })
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "PostgreSQL prune terminal transition omitted its event".to_owned())?;
    event_journal
        .append_record(event)
        .map(|_| ())
        .map_err(|error| error.to_string())
}
