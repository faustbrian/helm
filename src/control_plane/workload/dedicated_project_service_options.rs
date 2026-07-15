use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::engine::BindMount;
use std::collections::BTreeMap;

/// Complete planning inputs for one project-owned infrastructure container.
pub(crate) struct DedicatedProjectServiceOptions<'operation> {
    pub(crate) service: &'operation ServiceExecutionPlan,
    pub(crate) generated_environment: Option<&'operation BTreeMap<String, String>>,
    pub(crate) generated_command: Option<&'operation [String]>,
    pub(crate) generated_configuration_mount: Option<&'operation BindMount>,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) platform: &'operation str,
    pub(crate) network_name: &'operation str,
}
