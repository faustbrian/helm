use super::ProjectBackupExecutionResult;
use super::ipc::{IpcEventJournal, IpcEventKind, IpcOutputStream};
use super::record_ipc_event::record_ipc_event;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::{
    DaemonOperationStatus, DaemonOperationTransitionOptions, StateStore,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// Persists verified recovery evidence before publishing terminal completion.
pub(crate) fn publish_project_backup_result<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    result: ProjectBackupExecutionResult,
    now_unix_seconds: i64,
) -> Result<(), String>
where
    Store: StateStore,
{
    let (operation_id, outcome) = result.into_parts();
    let (kind, status) = match outcome {
        Ok(backup) => {
            let evidence = serde_json::json!({
                "artifact_sha256": backup.artifact_sha256(),
                "artifact_size_bytes": backup.artifact_size_bytes(),
                "recovery_point": backup.reference(),
            });
            record_ipc_event(
                control_plane,
                event_journal,
                &operation_id,
                IpcEventKind::Output {
                    stream: IpcOutputStream::Stdout,
                    data_base64: STANDARD.encode(evidence.to_string()),
                },
            )?;

            (IpcEventKind::Completed, DaemonOperationStatus::Completed)
        }
        Err(message) => (
            IpcEventKind::Failed {
                code: "project_backup_failed".to_owned(),
                message,
            },
            DaemonOperationStatus::Failed,
        ),
    };
    let kind_json = serde_json::to_string(&kind)
        .map_err(|error| format!("failed to encode project backup event: {error}"))?;
    let event = control_plane
        .transition_daemon_operation(DaemonOperationTransitionOptions {
            operation_id: &operation_id,
            expected: DaemonOperationStatus::Running,
            next: status,
            updated_at_unix_seconds: now_unix_seconds,
            event_kind_json: Some(&kind_json),
            event_retention_limit: event_journal.capacity(),
        })
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "project backup terminal transition omitted its event".to_owned())?;
    event_journal
        .append_record(event)
        .map(|_| ())
        .map_err(|error| error.to_string())
}
