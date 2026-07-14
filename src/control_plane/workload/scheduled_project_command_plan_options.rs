use crate::control_plane::{ProjectIdentity, ServiceIdentity};
use std::collections::BTreeMap;
use std::time::Duration;

/// Complete inputs for one daemon-timed command in an application runtime.
pub(crate) struct ScheduledProjectCommandPlanOptions {
    pub(crate) project: ProjectIdentity,
    pub(crate) service: ServiceIdentity,
    pub(crate) application_service: ServiceIdentity,
    pub(crate) arguments: Vec<String>,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) timeout: Duration,
}
