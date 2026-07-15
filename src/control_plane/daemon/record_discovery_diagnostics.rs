use super::ipc::{IpcDiagnostic, IpcEventJournal, IpcEventKind};
use super::record_ipc_event::record_ipc_event;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::StateStore;

pub(super) const DISCOVERY_OPERATION_ID: &str = "project-discovery";

/// Persists one complete changed discovery diagnostic snapshot.
pub(crate) fn record_discovery_diagnostics<Store>(
    control_plane: &mut ControlPlane<Store>,
    event_journal: &mut IpcEventJournal,
    diagnostics: &[IpcDiagnostic],
) -> Result<(), String>
where
    Store: StateStore,
{
    record_ipc_event(
        control_plane,
        event_journal,
        DISCOVERY_OPERATION_ID,
        IpcEventKind::Diagnostics {
            diagnostics: diagnostics.to_vec(),
        },
    )?;

    Ok(())
}
