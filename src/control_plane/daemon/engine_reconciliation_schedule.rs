use super::DiscoveryReconciliationResult;
use crate::control_plane::application::DesiredRegistry;

/// Fail-closed permission and due state for Engine-side reconciliation.
#[derive(Default)]
pub(crate) struct EngineReconciliationSchedule {
    permitted: bool,
    due: bool,
    desired_registry: Option<DesiredRegistry>,
}

impl EngineReconciliationSchedule {
    /// Replaces permission from the latest complete discovery attempt.
    pub(crate) fn observe(&mut self, reconciliation: &DiscoveryReconciliationResult) {
        self.permitted = reconciliation.was_applied();
        self.due = self.permitted;
        if let Some(registry) = reconciliation.registry() {
            self.desired_registry = Some(registry.clone());
        }
    }

    pub(crate) const fn may_reconcile(&self) -> bool {
        self.permitted
    }

    pub(crate) const fn is_due(&self) -> bool {
        self.due
    }

    /// Returns the last complete validated registry, including while blocked.
    pub(crate) const fn desired_registry(&self) -> Option<&DesiredRegistry> {
        self.desired_registry.as_ref()
    }

    /// Records successful convergence or a durable non-Engine conflict.
    pub(crate) fn complete(&mut self) {
        self.due = false;
    }
}
