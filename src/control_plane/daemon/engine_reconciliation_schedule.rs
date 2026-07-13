use super::DiscoveryReconciliationResult;
use crate::control_plane::application::DesiredRegistry;
use crate::control_plane::{ExecutionPlan, ServiceStrategyError, resolve_execution_plan};

/// Fail-closed permission and due state for Engine-side reconciliation.
#[derive(Default)]
pub(crate) struct EngineReconciliationSchedule {
    permitted: bool,
    due: bool,
    desired_registry: Option<DesiredRegistry>,
    execution_plan: Option<ExecutionPlan>,
}

impl EngineReconciliationSchedule {
    /// Replaces permission from the latest complete discovery attempt.
    pub(crate) fn observe(
        &mut self,
        reconciliation: &DiscoveryReconciliationResult,
    ) -> Result<(), ServiceStrategyError> {
        if let Some(registry) = reconciliation.registry() {
            let execution_plan = resolve_execution_plan(registry)?;
            self.desired_registry = Some(registry.clone());
            self.execution_plan = Some(execution_plan);
            self.permitted = true;
            self.due = true;
        } else {
            self.permitted = false;
            self.due = false;
        }

        Ok(())
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

    pub(crate) const fn execution_plan(&self) -> Option<&ExecutionPlan> {
        self.execution_plan.as_ref()
    }

    /// Records successful convergence or a durable non-Engine conflict.
    pub(crate) fn complete(&mut self) {
        self.due = false;
    }
}
