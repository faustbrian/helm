use super::ipc::{IpcDiagnostic, IpcEventJournal, IpcEventJournalError, IpcEventKind};
use super::record_discovery_diagnostics::DISCOVERY_OPERATION_ID;

/// Restores the latest complete automatic-discovery diagnostic snapshot.
pub(crate) fn restore_discovery_diagnostics(
    event_journal: &IpcEventJournal,
) -> Result<Vec<IpcDiagnostic>, IpcEventJournalError> {
    Ok(event_journal
        .events_after(None)?
        .into_iter()
        .rev()
        .find_map(|event| {
            if event.operation_id() != DISCOVERY_OPERATION_ID {
                return None;
            }
            let IpcEventKind::Diagnostics { diagnostics } = event.kind() else {
                return None;
            };

            Some(diagnostics.clone())
        })
        .unwrap_or_default())
}
