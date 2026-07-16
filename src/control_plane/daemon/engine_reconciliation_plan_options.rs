use crate::control_plane::ExecutionPlan;
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::project_infrastructure::PreparedProjectService;
use crate::control_plane::state::{ManagedEnvironmentRecord, ResourceRecord};

/// Complete host and durable-state inputs for one Engine plan.
pub(crate) struct EngineReconciliationPlanOptions<'operation> {
    pub(crate) execution: &'operation ExecutionPlan,
    pub(crate) prepared_shared_services: &'operation [(String, String)],
    pub(crate) prepared_project_services: &'operation [PreparedProjectService],
    pub(crate) shared_routes: &'operation [GatewayRoute],
    pub(crate) managed_environments: &'operation [ManagedEnvironmentRecord],
    pub(crate) durable_resources: &'operation [ResourceRecord],
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) platform: &'operation str,
    pub(crate) container_user: &'operation str,
    pub(crate) network_name: &'operation str,
    pub(crate) internal_http_port: u16,
}
