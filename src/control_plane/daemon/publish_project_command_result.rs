use super::ProjectCommandExecutionResult;
use super::ipc::{IpcEventJournal, IpcEventKind, IpcOutputStream};
use super::record_ipc_event::record_ipc_event;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::{
    DaemonOperationStatus, DaemonOperationTransitionOptions, StateStore,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

const OUTPUT_CHUNK_BYTES: usize = 24 * 1024;

/// Persists binary-safe command output in order before its terminal event.
pub(crate) fn publish_project_command_result<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    result: ProjectCommandExecutionResult,
    now_unix_seconds: i64,
) -> Result<(), String>
where
    Store: StateStore,
{
    let (operation_id, outcome) = result.into_parts();
    match outcome {
        Ok(output) => {
            publish_output(
                control_plane,
                event_journal,
                &operation_id,
                IpcOutputStream::Stdout,
                output.stdout(),
            )?;
            publish_output(
                control_plane,
                event_journal,
                &operation_id,
                IpcOutputStream::Stderr,
                output.stderr(),
            )?;
            publish_terminal_event(
                control_plane,
                event_journal,
                &operation_id,
                IpcEventKind::Completed,
                DaemonOperationStatus::Completed,
                now_unix_seconds,
            )?;
        }
        Err(error) => {
            publish_terminal_event(
                control_plane,
                event_journal,
                &operation_id,
                IpcEventKind::Failed {
                    code: "project_command_failed".to_owned(),
                    message: error.to_string(),
                },
                DaemonOperationStatus::Failed,
                now_unix_seconds,
            )?;
        }
    }

    Ok(())
}

fn publish_terminal_event<Store>(
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
        .map_err(|error| format!("failed to encode daemon event: {error}"))?;
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
        .ok_or_else(|| "terminal daemon operation transition omitted its event".to_owned())?;
    event_journal
        .append_record(event)
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn publish_output<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    operation_id: &str,
    stream: IpcOutputStream,
    bytes: &[u8],
) -> Result<(), String>
where
    Store: StateStore,
{
    for chunk in bytes.chunks(OUTPUT_CHUNK_BYTES) {
        record_ipc_event(
            control_plane,
            event_journal,
            operation_id,
            IpcEventKind::Output {
                stream,
                data_base64: STANDARD.encode(chunk),
            },
        )?;
    }

    Ok(())
}
