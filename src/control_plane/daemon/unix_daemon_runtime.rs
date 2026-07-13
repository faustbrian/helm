use super::{
    DaemonIterationResult, DiscoveryScheduler, SingletonLease, UnixDaemonRuntimeError,
    UnixDaemonRuntimeOptions, dispatch_daemon_request, reconcile_watched_roots,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::UnixIpcListener;
use crate::control_plane::state::SqliteStateStore;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// One authoritative Unix daemon owning state, scheduling, lease, and IPC.
pub(crate) struct UnixDaemonRuntime {
    _lease: SingletonLease,
    listener: UnixIpcListener,
    control_plane: ControlPlane<SqliteStateStore>,
    scheduler: DiscoveryScheduler,
    options: UnixDaemonRuntimeOptions,
}

impl UnixDaemonRuntime {
    /// Acquires exclusive ownership and opens every durable runtime boundary.
    pub(crate) fn new(
        options: UnixDaemonRuntimeOptions,
        now: Instant,
    ) -> Result<Self, UnixDaemonRuntimeError> {
        validate_options(&options)?;
        let runtime_directory = options
            .socket_path
            .parent()
            .ok_or_else(|| invalid("daemon socket path must have a parent directory"))?;
        prepare_runtime_directory(runtime_directory)?;
        let lease = SingletonLease::acquire(&options.lease_path)?;
        remove_stale_socket(&options.socket_path)?;
        let listener = UnixIpcListener::bind(&options.socket_path)?;
        listener.set_nonblocking(true)?;
        let store = SqliteStateStore::open(&options.state_database_path)?;
        let scheduler = DiscoveryScheduler::new(now, options.scheduler_options);

        Ok(Self {
            _lease: lease,
            listener,
            control_plane: ControlPlane::new(store),
            scheduler,
            options,
        })
    }

    /// Performs due reconciliation and serves at most one pending IPC request.
    pub(crate) fn run_iteration(
        &mut self,
        now: Instant,
        now_unix_seconds: i64,
    ) -> Result<DaemonIterationResult, UnixDaemonRuntimeError> {
        if now_unix_seconds < 0 {
            return Err(invalid("daemon wall-clock time must not be negative"));
        }

        let scan_reason = self.scheduler.take_due(now);
        let reconciliation = scan_reason
            .map(|_reason| {
                reconcile_watched_roots(
                    &mut self.control_plane,
                    self.options.discovery_options,
                    now_unix_seconds,
                )
            })
            .transpose()?;
        let request = self.listener.try_serve_next(|request| {
            dispatch_daemon_request(
                &mut self.control_plane,
                self.options.discovery_options,
                request,
                now_unix_seconds,
            )
        })?;

        Ok(DaemonIterationResult::new(
            scan_reason,
            reconciliation,
            request,
        ))
    }

    /// Records a native filesystem notification for debounced convergence.
    pub(crate) fn record_filesystem_event(&mut self, now: Instant) {
        self.scheduler.record_filesystem_event(now);
    }

    /// Runs until the process is stopped by the per-user service manager.
    #[expect(
        clippy::infinite_loop,
        reason = "the singleton daemon is a login-lifetime service"
    )]
    pub(crate) fn run_forever(&mut self) {
        loop {
            let now = Instant::now();
            if let Err(error) = self.run_iteration(now, unix_time_seconds()) {
                tracing::error!(error = %error, "singleton daemon iteration failed");
                self.scheduler.record_filesystem_event(Instant::now());
            }
            let until_scan = self
                .scheduler
                .next_deadline()
                .saturating_duration_since(now);
            std::thread::sleep(self.options.idle_poll_interval.min(until_scan));
        }
    }
}

fn validate_options(options: &UnixDaemonRuntimeOptions) -> Result<(), UnixDaemonRuntimeError> {
    if options.idle_poll_interval.is_zero() {
        return Err(invalid(
            "daemon idle poll interval must be greater than zero",
        ));
    }
    if !options.state_database_path.is_absolute()
        || !options.lease_path.is_absolute()
        || !options.socket_path.is_absolute()
    {
        return Err(invalid("daemon runtime paths must be absolute"));
    }
    let socket_parent = options.socket_path.parent();
    if options.state_database_path.parent() != socket_parent
        || options.lease_path.parent() != socket_parent
    {
        return Err(invalid(
            "daemon database, lease, and socket must share one runtime directory",
        ));
    }
    if options.state_database_path == options.lease_path
        || options.state_database_path == options.socket_path
        || options.lease_path == options.socket_path
    {
        return Err(invalid("daemon runtime paths must be distinct"));
    }

    Ok(())
}

fn prepare_runtime_directory(path: &Path) -> Result<(), UnixDaemonRuntimeError> {
    std::fs::create_dir_all(path).map_err(|source| file_system_error("create", path, source))?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|source| file_system_error("inspect", path, source))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid(
            "daemon runtime directory must be a real directory, not a link",
        ));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|source| file_system_error("secure", path, source))
}

fn remove_stale_socket(path: &Path) -> Result<(), UnixDaemonRuntimeError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(path)
            .map_err(|source| file_system_error("remove stale socket", path, source)),
        Ok(_) => Err(invalid(
            "daemon socket path exists but is not an owned Unix socket",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(file_system_error("inspect socket", path, source)),
    }
}

fn unix_time_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(i64::MAX)
}

fn invalid(detail: impl Into<String>) -> UnixDaemonRuntimeError {
    UnixDaemonRuntimeError::InvalidOptions {
        detail: detail.into(),
    }
}

fn file_system_error(
    action: &'static str,
    path: &Path,
    source: std::io::Error,
) -> UnixDaemonRuntimeError {
    UnixDaemonRuntimeError::FileSystem {
        action,
        path: path.to_path_buf(),
        source,
    }
}
