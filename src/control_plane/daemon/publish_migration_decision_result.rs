use super::MigrationDecisionExecutionResult;
use super::ipc::{IpcEventJournal, IpcEventKind, IpcOutputStream};
use super::record_ipc_event::record_ipc_event;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::migration::MigrationExecutionResult;
use crate::control_plane::state::{
    DaemonOperationStatus, DaemonOperationTransitionOptions, StateStore,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// Publishes exact terminal evidence for one explicit migration decision.
pub(crate) fn publish_migration_decision_result<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    result: MigrationDecisionExecutionResult,
    now_unix_seconds: i64,
) -> Result<(), String>
where
    Store: StateStore,
{
    let (operation, outcome) = result.into_parts();
    let (kind, status) = match outcome {
        Ok(migration_state) => {
            let state = match migration_state {
                MigrationExecutionResult::AwaitingConfirmation => "awaiting_confirmation",
                MigrationExecutionResult::Confirmed => "confirmed",
                MigrationExecutionResult::RolledBack => "rolled_back",
            };
            let evidence = serde_json::json!({
                "migration_id": operation.migration_id(),
                "state": state,
            });
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
                code: "migration_decision_failed".to_owned(),
                message,
            },
            DaemonOperationStatus::Failed,
        ),
    };
    let kind_json = serde_json::to_string(&kind)
        .map_err(|error| format!("failed to encode migration decision event: {error}"))?;
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
        .ok_or_else(|| "migration decision terminal transition omitted its event".to_owned())?;
    event_journal
        .append_record(event)
        .map(|_| ())
        .map_err(|error| error.to_string())
}
