use super::ImmutableProjectApplicationPlan;
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::state::ManagedEnvironmentRecord;

/// Complete inputs for one supervised process tied to an application runtime.
pub(crate) struct ProjectProcessOperationOptions<'operation> {
    pub(crate) service: &'operation ServiceExecutionPlan,
    pub(crate) application_service: &'operation ServiceExecutionPlan,
    pub(crate) application: &'operation ImmutableProjectApplicationPlan,
    pub(crate) managed_environment: ManagedEnvironmentRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) platform: &'operation str,
    pub(crate) container_user: &'operation str,
    pub(crate) network_name: &'operation str,
}
