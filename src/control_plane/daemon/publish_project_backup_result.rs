use super::ProjectBackupExecutionResult;
use super::ipc::{IpcEventJournal, IpcEventKind, IpcOutputStream};
use super::record_ipc_event::record_ipc_event;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::{
    DaemonOperationStatus, DaemonOperationTransitionOptions, RecoveryPointRecord,
    RecoveryPointRecordOptions, StateStore,
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
    let (operation, created_at_unix_seconds, outcome) = result.into_parts();
    let operation_id = operation.operation_id().to_owned();
    let (kind, status) = match outcome {
        Ok(backup) => {
            let recovery_point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
                recovery_point_id: operation_id.clone(),
                project_id: operation.project_id().to_owned(),
                service_id: operation.service_id().to_owned(),
                logical_resource_id: operation.logical_resource_id().to_owned(),
                resource_kind: operation.kind().to_owned(),
                compatibility_fingerprint: operation.compatibility_fingerprint().to_owned(),
                reference: backup.reference().to_owned(),
                artifact_sha256: backup.artifact_sha256().to_owned(),
                artifact_size_bytes: backup.artifact_size_bytes(),
                created_at_unix_seconds,
                verified_at_unix_seconds: now_unix_seconds,
            });
            let catalog = recovery_point
                .map_err(|error| format!("failed to construct recovery evidence: {error}"))
                .and_then(|recovery_point| {
                    control_plane
                        .record_recovery_point(&recovery_point)
                        .map_err(|error| format!("failed to persist recovery evidence: {error}"))
                });
            if let Err(message) = catalog {
                return publish_terminal(
                    control_plane,
                    event_journal,
                    &operation_id,
                    IpcEventKind::Failed {
                        code: "project_backup_catalog_failed".to_owned(),
                        message,
                    },
                    DaemonOperationStatus::Failed,
                    now_unix_seconds,
                );
            }
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
    publish_terminal(
        control_plane,
        event_journal,
        &operation_id,
        kind,
        status,
        now_unix_seconds,
    )
}

fn publish_terminal<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    operation_id: &str,
    kind: IpcEventKind,
    status: DaemonOperationStatus,
    now_unix_seconds: i64,
) -> Result<(), String>
where
    Store: StateStore,
{
    let kind_json = serde_json::to_string(&kind)
        .map_err(|error| format!("failed to encode project backup event: {error}"))?;
    let event = control_plane
        .transition_daemon_operation(DaemonOperationTransitionOptions {
            operation_id,
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
