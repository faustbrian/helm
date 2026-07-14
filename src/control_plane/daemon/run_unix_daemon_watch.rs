use super::{
    DiscoveryReconciliationResult, UnixDaemonRuntimeError, UnixDaemonWatchOptions,
    run_unix_daemon_watch_with_resolver,
};
use crate::control_plane::gateway::SystemLocalhostResolver;

/// Configures authoritative roots and runs the single Unix control plane.
pub(crate) fn run_unix_daemon_watch(
    options: &UnixDaemonWatchOptions,
) -> Result<Option<DiscoveryReconciliationResult>, UnixDaemonRuntimeError> {
    run_unix_daemon_watch_with_resolver(options, &SystemLocalhostResolver)
}
