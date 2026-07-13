use super::ipc::{IpcEventJournal, IpcEventKind};
use super::{ProjectCommandQueue, QueuedProjectCommand};
use crate::control_plane::state::{
    DaemonOperationStatus, DaemonOperationTransitionOptions, StateStore, StateStoreError,
};

/// Restores queued commands and terminalizes ambiguous in-flight work.
pub(crate) fn restore_project_command_operations<Store>(
    store: &mut Store,
    now_unix_seconds: i64,
) -> Result<ProjectCommandQueue, StateStoreError>
where
    Store: StateStore,
{
    let operations = store.active_daemon_operations()?;
    let mut queue = ProjectCommandQueue::default();
    let event_capacity = IpcEventJournal::default().capacity();
    for operation in operations {
        if operation.status() == DaemonOperationStatus::Running {
            fail_operation(
                store,
                operation.operation_id(),
                DaemonOperationStatus::Running,
                "project_command_interrupted",
                "the daemon restarted while the project command was running; the command was not replayed",
                now_unix_seconds,
                event_capacity,
            )?;

            continue;
        }
        if operation.kind() != "project_command" {
            fail_operation(
                store,
                operation.operation_id(),
                DaemonOperationStatus::Queued,
                "operation_payload_invalid",
                "the queued daemon operation kind is unsupported by this build",
                now_unix_seconds,
                event_capacity,
            )?;

            continue;
        }
        let queued = QueuedProjectCommand::from_payload_json(
            operation.operation_id().to_owned(),
            operation.payload_json(),
        );
        let queued = match queued {
            Ok(queued) => queued,
            Err(error) => {
                fail_operation(
                    store,
                    operation.operation_id(),
                    DaemonOperationStatus::Queued,
                    "operation_payload_invalid",
                    &error,
                    now_unix_seconds,
                    event_capacity,
                )?;

                continue;
            }
        };
        if let Err(error) = queue.enqueue(queued) {
            fail_operation(
                store,
                operation.operation_id(),
                DaemonOperationStatus::Queued,
                "project_command_queue_unavailable",
                &error.to_string(),
                now_unix_seconds,
                event_capacity,
            )?;
        }
    }

    Ok(queue)
}

fn fail_operation<Store>(
    store: &mut Store,
    operation_id: &str,
    expected: DaemonOperationStatus,
    code: &str,
    message: &str,
    now_unix_seconds: i64,
    event_capacity: usize,
) -> Result<(), StateStoreError>
where
    Store: StateStore,
{
    let kind_json = serde_json::to_string(&IpcEventKind::Failed {
        code: code.to_owned(),
        message: message.to_owned(),
    })
    .map_err(|error| StateStoreError::InvalidDaemonOperation {
        detail: format!("failed to encode restart terminal event: {error}"),
    })?;
    drop(
        store.transition_daemon_operation(DaemonOperationTransitionOptions {
            operation_id,
            expected,
            next: DaemonOperationStatus::Failed,
            updated_at_unix_seconds: now_unix_seconds,
            event_kind_json: Some(&kind_json),
            event_retention_limit: event_capacity,
        })?,
    );

    Ok(())
}
