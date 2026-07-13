use crate::control_plane::ExecutionPlan;
use crate::control_plane::state::ManagedEnvironmentRecord;

/// Complete host and durable-state inputs for one Engine plan.
pub(crate) struct EngineReconciliationPlanOptions<'operation> {
    pub(crate) execution: &'operation ExecutionPlan,
    pub(crate) managed_environments: &'operation [ManagedEnvironmentRecord],
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) platform: &'operation str,
    pub(crate) network_name: &'operation str,
    pub(crate) internal_http_port: u16,
}
