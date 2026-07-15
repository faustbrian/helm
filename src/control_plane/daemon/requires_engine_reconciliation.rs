use super::ipc::{IpcPayload, IpcRequest};
use super::is_valid_certificate_generation;

/// Returns whether a daemon request must reconcile the retained Engine plan.
pub(crate) fn requires_engine_reconciliation(request: &IpcRequest) -> bool {
    matches!(
        request.payload(),
        IpcPayload::ActivateGatewayCertificate { generation }
            if is_valid_certificate_generation(generation)
    )
}
