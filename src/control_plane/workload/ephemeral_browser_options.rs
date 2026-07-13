use crate::control_plane::ServiceExecutionPlan;

/// Complete inputs for one command-scoped browser sidecar.
pub(crate) struct EphemeralBrowserOptions<'operation> {
    pub(crate) service: &'operation ServiceExecutionPlan,
    pub(crate) operation_id: &'operation str,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) platform: &'operation str,
    pub(crate) network_name: &'operation str,
}
