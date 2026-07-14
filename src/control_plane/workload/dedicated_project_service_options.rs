use crate::control_plane::ServiceExecutionPlan;
use std::collections::BTreeMap;

/// Complete planning inputs for one project-owned infrastructure container.
pub(crate) struct DedicatedProjectServiceOptions<'operation> {
    pub(crate) service: &'operation ServiceExecutionPlan,
    pub(crate) generated_environment: Option<&'operation BTreeMap<String, String>>,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) platform: &'operation str,
    pub(crate) network_name: &'operation str,
}
