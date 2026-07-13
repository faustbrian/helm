use super::DiscoveryReconciliationResult;

/// Fail-closed permission and due state for Engine-side reconciliation.
#[derive(Default)]
pub(crate) struct EngineReconciliationSchedule {
    permitted: bool,
    due: bool,
}

impl EngineReconciliationSchedule {
    /// Replaces permission from the latest complete discovery attempt.
    pub(crate) fn observe(&mut self, reconciliation: &DiscoveryReconciliationResult) {
        self.permitted = reconciliation.was_applied();
        self.due = self.permitted;
    }

    pub(crate) const fn may_reconcile(&self) -> bool {
        self.permitted
    }

    pub(crate) const fn is_due(&self) -> bool {
        self.due
    }

    /// Records successful convergence or a durable non-Engine conflict.
    pub(crate) fn complete(&mut self) {
        self.due = false;
    }
}
