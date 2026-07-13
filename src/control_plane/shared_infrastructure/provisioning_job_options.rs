use crate::control_plane::engine::ContainerCreateOptions;
use std::time::Duration;

/// Inputs required to run one owned disposable provisioning container.
pub(crate) struct ProvisioningJobOptions<'operation> {
    pub(crate) request: &'operation ContainerCreateOptions,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) timeout: Duration,
}
