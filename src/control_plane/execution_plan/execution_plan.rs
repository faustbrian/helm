use super::ServiceExecutionPlan;

/// Complete dependency-ordered service operations for one validated registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionPlan {
    services: Vec<ServiceExecutionPlan>,
}

impl ExecutionPlan {
    pub(super) const fn new(services: Vec<ServiceExecutionPlan>) -> Self {
        Self { services }
    }

    pub(crate) fn services(&self) -> &[ServiceExecutionPlan] {
        &self.services
    }
}
