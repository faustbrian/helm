use crate::control_plane::engine::{ContainerCreateOptions, ObservedContainer};
use std::num::NonZeroUsize;
use std::time::Duration;

/// Inputs for one bounded batch of independent provisioning jobs.
pub(crate) struct ProvisioningJobsRunOptions<'operation> {
    pub(crate) requests: &'operation [ContainerCreateOptions],
    pub(crate) observed: &'operation [ObservedContainer],
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) timeout: Duration,
    pub(crate) concurrency: NonZeroUsize,
}
