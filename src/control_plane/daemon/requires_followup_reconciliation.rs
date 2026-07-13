use super::ipc::{IpcPayload, IpcRequest};

/// Returns whether a daemon mutation must pass through a fresh complete scan.
pub(crate) fn requires_followup_reconciliation(request: &IpcRequest) -> bool {
    matches!(
        request.payload(),
        IpcPayload::Reconcile | IpcPayload::AdoptProject { .. }
    )
}
