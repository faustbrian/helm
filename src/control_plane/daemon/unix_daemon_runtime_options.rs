use super::{DiscoverySchedulerOptions, ProjectDiscoveryOptions};
use std::path::PathBuf;
use std::time::Duration;

/// Complete paths, bounds, and timing for one Unix singleton daemon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnixDaemonRuntimeOptions {
    pub(crate) state_database_path: PathBuf,
    pub(crate) lease_path: PathBuf,
    pub(crate) socket_path: PathBuf,
    pub(crate) discovery_options: ProjectDiscoveryOptions,
    pub(crate) scheduler_options: DiscoverySchedulerOptions,
    pub(crate) idle_poll_interval: Duration,
}
