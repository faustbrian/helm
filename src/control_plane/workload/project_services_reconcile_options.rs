use crate::control_plane::engine::{ContainerCreateOptions, ObservedContainer};
use std::num::NonZeroUsize;

/// Complete mutation boundary for independent dedicated project services.
pub(crate) struct ProjectServicesReconcileOptions<'request> {
    pub(crate) requests: &'request [ContainerCreateOptions],
    pub(crate) observed: &'request [ObservedContainer],
    pub(crate) installation_id: &'request str,
    pub(crate) schema_version: u32,
    pub(crate) concurrency: NonZeroUsize,
}
