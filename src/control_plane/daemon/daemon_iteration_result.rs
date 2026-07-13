use super::{DiscoveryReconciliationResult, DiscoveryScanReason};
use crate::control_plane::daemon::ipc::IpcRequest;

/// Observable work completed by one bounded singleton daemon iteration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DaemonIterationResult {
    scan_reason: Option<DiscoveryScanReason>,
    reconciliation: Option<DiscoveryReconciliationResult>,
    request: Option<IpcRequest>,
}

impl DaemonIterationResult {
    pub(super) const fn new(
        scan_reason: Option<DiscoveryScanReason>,
        reconciliation: Option<DiscoveryReconciliationResult>,
        request: Option<IpcRequest>,
    ) -> Self {
        Self {
            scan_reason,
            reconciliation,
            request,
        }
    }

    pub(crate) const fn scan_reason(&self) -> Option<DiscoveryScanReason> {
        self.scan_reason
    }

    pub(crate) const fn reconciliation(&self) -> Option<&DiscoveryReconciliationResult> {
        self.reconciliation.as_ref()
    }

    pub(crate) const fn request(&self) -> Option<&IpcRequest> {
        self.request.as_ref()
    }
}
