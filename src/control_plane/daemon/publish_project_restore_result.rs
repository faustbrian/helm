use super::ProjectRestoreExecutionResult;
use super::ipc::{IpcEventJournal, IpcEventKind, IpcOutputStream};
use super::record_ipc_event::record_ipc_event;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::migration::MigrationExecutionResult;
use crate::control_plane::state::{
    DaemonOperationStatus, DaemonOperationTransitionOptions, StateStore,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// Publishes the operator-gated migration state without retiring its source.
pub(crate) fn publish_project_restore_result<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    result: ProjectRestoreExecutionResult,
    now_unix_seconds: i64,
) -> Result<(), String>
where
    Store: StateStore,
{
    let (operation, outcome) = result.into_parts();
    let operation_id = operation.operation_id().to_owned();
    let (kind, status) = match outcome {
        Ok(migration_state) => {
            let state = match migration_state {
                MigrationExecutionResult::AwaitingConfirmation => "awaiting_confirmation",
                MigrationExecutionResult::Confirmed => "confirmed",
                MigrationExecutionResult::RolledBack => "rolled_back",
            };
            let evidence = if operation.dump_file().is_some() {
                serde_json::json!({
                    "service": operation.service_id(),
                    "state": "restored",
                    "reset": operation.resets_database(),
                })
            } else {
                serde_json::json!({
                    "migration_id": operation_id,
                    "state": state,
                })
            };
            record_ipc_event(
                control_plane,
                event_journal,
                operation.operation_id(),
                IpcEventKind::Output {
                    stream: IpcOutputStream::Stdout,
                    data_base64: STANDARD.encode(evidence.to_string()),
                },
            )?;

            (IpcEventKind::Completed, DaemonOperationStatus::Completed)
        }
        Err(message) => (
            IpcEventKind::Failed {
                code: "project_restore_failed".to_owned(),
                message,
            },
            DaemonOperationStatus::Failed,
        ),
    };
    let kind_json = serde_json::to_string(&kind)
        .map_err(|error| format!("failed to encode project restore event: {error}"))?;
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
        .ok_or_else(|| "project restore terminal transition omitted its event".to_owned())?;
    event_journal
        .append_record(event)
        .map(|_| ())
        .map_err(|error| error.to_string())
}
