use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::state::ManagedEnvironmentRecord;

/// Complete daemon inputs for one immutable project application operation.
pub(crate) struct ImmutableProjectApplicationOptions<'operation> {
    pub(crate) service: &'operation ServiceExecutionPlan,
    pub(crate) managed_environment: ManagedEnvironmentRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) platform: &'operation str,
    pub(crate) network_name: &'operation str,
    pub(crate) internal_http_port: u16,
}
