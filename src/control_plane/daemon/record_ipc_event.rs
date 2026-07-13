use super::ipc::{IpcEvent, IpcEventJournal, IpcEventKind};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::StateStore;

/// Persists one lifecycle transition before publishing it to live subscribers.
pub(super) fn record_ipc_event<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    operation_id: &str,
    kind: IpcEventKind,
) -> Result<IpcEvent, String>
where
    Store: StateStore,
{
    let kind_json = serde_json::to_string(&kind)
        .map_err(|error| format!("failed to encode daemon event: {error}"))?;
    let record = control_plane
        .append_daemon_event(operation_id, &kind_json, event_journal.capacity())
        .map_err(|error| error.to_string())?;

    event_journal
        .append_record(record)
        .map_err(|error| error.to_string())
}
