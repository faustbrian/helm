use super::{
    DiscoveryReconciliationResult, DiscoverySchedulerOptions, ProjectDiscoveryOptions,
    UnixDaemonRuntime, UnixDaemonRuntimeError, UnixDaemonRuntimeOptions, UnixDaemonWatchOptions,
};
use crate::control_plane::gateway::{LocalhostResolver, verify_stackctl_localhost_resolution};
use crate::control_plane::state::{SqliteStateStore, StateStore};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Verifies host routing before creating or changing authoritative daemon state.
pub(crate) fn run_unix_daemon_watch_with_resolver(
    options: &UnixDaemonWatchOptions,
    resolver: &impl LocalhostResolver,
) -> Result<Option<DiscoveryReconciliationResult>, UnixDaemonRuntimeError> {
    verify_stackctl_localhost_resolution(resolver)?;
    std::fs::create_dir_all(&options.runtime_directory).map_err(|source| {
        UnixDaemonRuntimeError::FileSystem {
            action: "create",
            path: options.runtime_directory.clone(),
            source,
        }
    })?;
    let database_path = options.runtime_directory.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path)?;
    store.replace_watched_roots(&options.watched_roots)?;
    drop(store);
    let runtime_options = UnixDaemonRuntimeOptions {
        state_database_path: database_path,
        lease_path: options.runtime_directory.join("daemon.lock"),
        socket_path: options.runtime_directory.join("daemon.sock"),
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        scheduler_options: DiscoverySchedulerOptions::new(
            Duration::from_millis(250),
            Duration::from_secs(2),
            options.periodic_rescan,
        )?,
        idle_poll_interval: Duration::from_millis(50),
    };
    let now = Instant::now();
    let mut runtime = UnixDaemonRuntime::new(runtime_options, now)?;
    if options.once {
        return runtime
            .run_iteration(now, current_unix_seconds())
            .map(|result| result.reconciliation().cloned());
    }

    runtime.run_forever();
    Ok(None)
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(i64::MAX)
}
