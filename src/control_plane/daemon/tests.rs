use super::{
    DiscoveryScanReason, DiscoveryScheduler, DiscoverySchedulerOptions, EngineConnectionFuture,
    EngineConnectionOutcome, EngineConnectionSupervisor, EngineConnector, ProjectDiscoveryOptions,
    RetryBackoff, RetryBackoffOptions, SingletonLease, discover_project_sources,
    dispatch_daemon_request, reconcile_watched_roots,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::{IpcPayload, IpcRequest, IpcResponse, IpcResult};
use crate::control_plane::state::{SqliteStateStore, StateStore};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn engine_connection_retries_only_after_backoff_and_recovers() {
    use crate::control_plane::engine::EngineError;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct RecordingConnector {
        attempts: Arc<AtomicUsize>,
        outcomes: VecDeque<Result<(), EngineError>>,
    }

    impl EngineConnector for RecordingConnector {
        type Engine = ();

        fn connect<'operation>(
            &'operation mut self,
            _endpoint: &'operation Path,
        ) -> EngineConnectionFuture<'operation, Self::Engine> {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            let outcome = self.outcomes.pop_front().unwrap_or(Ok(()));
            Box::pin(async move { outcome })
        }
    }

    let attempts = Arc::new(AtomicUsize::new(0));
    let connector = RecordingConnector {
        attempts: Arc::clone(&attempts),
        outcomes: VecDeque::from([
            Err(EngineError::Backend {
                detail: "Docker Desktop is starting".to_owned(),
            }),
            Ok(()),
        ]),
    };
    let retry = RetryBackoff::new(
        "engine:docker",
        RetryBackoffOptions::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("retry options"),
    )
    .expect("retry backoff");
    let mut supervisor =
        EngineConnectionSupervisor::new(connector, PathBuf::from("/var/run/docker.sock"), retry)
            .expect("Engine supervisor");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("async runtime");
    let started_at = Instant::now();

    let failed = runtime.block_on(supervisor.poll(started_at));
    let EngineConnectionOutcome::Unavailable { retry, detail } = failed else {
        panic!("first Engine connection must be unavailable");
    };
    assert_eq!(retry.attempt(), 1);
    assert!(
        detail.contains("Docker Desktop is starting"),
        "unexpected connection failure: {detail}",
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 1);

    assert!(matches!(
        runtime.block_on(supervisor.poll(started_at + retry.duration() / 2)),
        EngineConnectionOutcome::BackingOff { .. }
    ));
    assert_eq!(attempts.load(Ordering::Relaxed), 1);

    assert_eq!(
        runtime.block_on(supervisor.poll(started_at + retry.duration())),
        EngineConnectionOutcome::Connected
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 2);
    assert!(supervisor.engine_mut().is_some());
}

#[test]
fn discovery_scheduler_coalesces_editor_events_and_bounds_continuous_writes() {
    let start = Instant::now();
    let options = DiscoverySchedulerOptions::new(
        Duration::from_millis(250),
        Duration::from_secs(2),
        Duration::from_secs(30),
    )
    .expect("scheduler options");
    let mut scheduler = DiscoveryScheduler::new(start, options);

    assert_eq!(
        scheduler.take_due(start),
        Some(DiscoveryScanReason::Initial)
    );

    scheduler.record_filesystem_event(start + Duration::from_secs(1));
    scheduler.record_filesystem_event(start + Duration::from_millis(1_100));
    assert_eq!(
        scheduler.next_deadline(),
        start + Duration::from_millis(1_350)
    );
    assert_eq!(
        scheduler.take_due(start + Duration::from_millis(1_349)),
        None
    );
    assert_eq!(
        scheduler.take_due(start + Duration::from_millis(1_350)),
        Some(DiscoveryScanReason::FilesystemEvents)
    );

    scheduler.record_filesystem_event(start + Duration::from_secs(2));
    for offset in [400_u64, 800, 1_200, 1_600, 2_000, 2_400] {
        scheduler.record_filesystem_event(start + Duration::from_millis(2_000 + offset));
    }
    assert_eq!(scheduler.next_deadline(), start + Duration::from_secs(4));
    assert_eq!(
        scheduler.take_due(start + Duration::from_secs(4)),
        Some(DiscoveryScanReason::FilesystemEvents)
    );
}

#[test]
fn discovery_scheduler_runs_periodic_correctness_scans_without_events() {
    let start = Instant::now();
    let options = DiscoverySchedulerOptions::new(
        Duration::from_millis(250),
        Duration::from_secs(2),
        Duration::from_secs(30),
    )
    .expect("scheduler options");
    let mut scheduler = DiscoveryScheduler::new(start, options);
    assert_eq!(
        scheduler.take_due(start),
        Some(DiscoveryScanReason::Initial)
    );

    assert_eq!(scheduler.next_deadline(), start + Duration::from_secs(30));
    assert_eq!(
        scheduler.take_due(start + Duration::from_secs(30)),
        Some(DiscoveryScanReason::Periodic)
    );
    assert_eq!(scheduler.next_deadline(), start + Duration::from_secs(60));
}

#[test]
fn retry_backoff_is_bounded_jittered_and_resets_after_recovery() {
    let options = RetryBackoffOptions::new(Duration::from_millis(500), Duration::from_secs(8))
        .expect("retry options");
    let mut engine =
        RetryBackoff::new("engine:docker-desktop", options).expect("engine retry backoff");
    let mut database =
        RetryBackoff::new("shared:postgres:17", options).expect("database retry backoff");

    let engine_delays = (0..8).map(|_| engine.next_delay()).collect::<Vec<_>>();
    let database_delays = (0..8).map(|_| database.next_delay()).collect::<Vec<_>>();

    assert_eq!(
        engine_delays
            .iter()
            .map(|delay| delay.attempt())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert!(
        engine_delays
            .iter()
            .all(|delay| delay.duration() <= Duration::from_secs(8))
    );
    assert_ne!(engine_delays, database_delays);

    let first = engine_delays[0];
    engine.reset();
    assert_eq!(engine.next_delay(), first);
}

#[test]
fn only_one_daemon_can_hold_a_user_lease() {
    let lock_path = temporary_lock_path("exclusive");
    let first = SingletonLease::acquire(&lock_path).expect("first singleton lease");

    let error = SingletonLease::acquire(&lock_path).expect_err("second lease must fail");

    assert_eq!(
        error.to_string(),
        format!("another Stackctl daemon owns '{}'", lock_path.display())
    );

    drop(first);
    remove_lock(&lock_path);
}

#[test]
fn released_daemon_lease_can_be_acquired_without_stale_pid_recovery() {
    let lock_path = temporary_lock_path("recovery");

    {
        let _first = SingletonLease::acquire(&lock_path).expect("first singleton lease");
    }

    let recovered = SingletonLease::acquire(&lock_path).expect("recovered singleton lease");
    let owner = std::fs::read_to_string(&lock_path).expect("read lease owner");

    assert_eq!(owner, std::process::id().to_string());

    drop(recovered);
    remove_lock(&lock_path);
}

#[cfg(unix)]
#[test]
fn daemon_lease_is_readable_and_writable_only_by_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let lock_path = temporary_lock_path("permissions");
    let lease = SingletonLease::acquire(&lock_path).expect("singleton lease");
    let mode = std::fs::metadata(&lock_path)
        .expect("lease metadata")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(mode, 0o600);

    drop(lease);
    remove_lock(&lock_path);
}

#[test]
fn watched_root_scan_discovers_nested_yaml_in_canonical_order() {
    let root = temporary_directory("discovery");
    let alpha = root.join("alpha");
    let zeta = root.join("nested/zeta");
    std::fs::create_dir_all(&alpha).expect("alpha directory");
    std::fs::create_dir_all(&zeta).expect("zeta directory");
    std::fs::write(
        alpha.join(".stackctl.yaml"),
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("alpha config");
    std::fs::write(
        zeta.join(".stackctl.yaml"),
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("zeta config");

    let report = discover_project_sources(
        &[root.clone(), root.clone()],
        ProjectDiscoveryOptions::bounded_defaults(),
    )
    .expect("project discovery");
    let sources = report.sources();

    assert_eq!(sources.len(), 2);
    assert!(report.issues().is_empty());
    assert_eq!(
        sources
            .iter()
            .map(|source| source.canonical_path())
            .collect::<Vec<_>>(),
        vec![
            alpha.canonicalize().expect("canonical alpha"),
            zeta.canonicalize().expect("canonical zeta"),
        ]
    );

    std::fs::remove_dir_all(&root).expect("remove discovery fixture");
}

#[test]
fn watched_root_scan_reports_all_toml_only_projects_without_loading_toml() {
    let root = temporary_directory("legacy-toml");
    for project in ["alpha", "zeta"] {
        let directory = root.join(project);
        std::fs::create_dir_all(&directory).expect("project directory");
        std::fs::write(directory.join(".stackctl.toml"), "project = 'legacy'\n")
            .expect("legacy config");
    }
    let valid = root.join("valid");
    std::fs::create_dir(&valid).expect("valid project directory");
    std::fs::write(
        valid.join(".stackctl.yaml"),
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("valid YAML config");

    let report = discover_project_sources(
        std::slice::from_ref(&root),
        ProjectDiscoveryOptions::bounded_defaults(),
    )
    .expect("bounded discovery");
    let diagnostics = report
        .issues()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(report.sources().len(), 1);
    assert_eq!(report.issues().len(), 2);
    assert!(diagnostics.contains("alpha/.stackctl.toml"));
    assert!(diagnostics.contains("zeta/.stackctl.toml"));
    assert!(diagnostics.contains("stackctl config migrate --to yaml"));

    std::fs::remove_dir_all(&root).expect("remove TOML fixture");
}

#[test]
fn incomplete_daemon_scan_preserves_the_last_complete_registry() {
    let root = temporary_directory("fail-closed-reconciliation");
    let project = root.join("bill");
    std::fs::create_dir(&project).expect("project directory");
    std::fs::write(
        project.join(".stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("project config");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_watched_roots(std::slice::from_ref(&root))
        .expect("persist watched root");
    let mut control_plane = ControlPlane::new(store);

    let applied = reconcile_watched_roots(
        &mut control_plane,
        ProjectDiscoveryOptions::bounded_defaults(),
        10_000,
    )
    .expect("complete scan");
    assert!(applied.was_applied());
    assert_eq!(applied.report().sources().len(), 1);
    assert_eq!(
        applied
            .registry()
            .expect("published registry")
            .projects()
            .len(),
        1
    );

    std::fs::remove_file(project.join(".stackctl.yaml")).expect("remove project config");
    let legacy = root.join("legacy");
    std::fs::create_dir(&legacy).expect("legacy directory");
    std::fs::write(legacy.join(".stackctl.toml"), "project = 'legacy'\n").expect("legacy config");

    let blocked = reconcile_watched_roots(
        &mut control_plane,
        ProjectDiscoveryOptions::bounded_defaults(),
        12_345,
    )
    .expect("incomplete scan is a durable diagnostic");
    assert!(!blocked.was_applied());
    assert_eq!(blocked.report().issues().len(), 1);

    drop(control_plane);
    let store = SqliteStateStore::open(&database_path).expect("reopen state store");
    let projects = store.projects().expect("load retained registry");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].project_name(), "bill");

    drop(store);
    std::fs::remove_dir_all(&root).expect("remove reconciliation fixture");
}

#[test]
fn daemon_reconcile_request_publishes_the_complete_watched_registry() {
    let root = temporary_directory("ipc-reconciliation");
    let project = root.join("bill");
    std::fs::create_dir(&project).expect("project directory");
    std::fs::write(
        project.join(".stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("project config");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_watched_roots(std::slice::from_ref(&root))
        .expect("persist watched root");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new("reconcile-42", IpcPayload::Reconcile);

    let response = dispatch_daemon_request(
        &mut control_plane,
        ProjectDiscoveryOptions::bounded_defaults(),
        &request,
        10_000,
    );

    assert_eq!(
        response,
        IpcResponse::success(
            "reconcile-42",
            IpcResult::Reconciled {
                project_count: 1,
                issue_count: 0,
                applied: true,
            },
        )
    );

    drop(control_plane);
    std::fs::remove_dir_all(&root).expect("remove reconciliation fixture");
}

#[cfg(unix)]
#[test]
fn singleton_unix_runtime_serves_ipc_and_runs_initial_reconciliation() {
    use super::{UnixDaemonRuntime, UnixDaemonRuntimeOptions};
    use crate::control_plane::daemon::ipc::{decode_response_frame, encode_frame};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let root = std::env::temp_dir().join(format!(
        "s8r-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("runtime test root");
    let project = root.join("bill");
    std::fs::create_dir(&project).expect("project directory");
    std::fs::write(
        project.join(".stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("project config");
    let runtime_directory = root.join("runtime");
    let database_path = runtime_directory.join("state.sqlite3");
    std::fs::create_dir(&runtime_directory).expect("runtime directory");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_watched_roots(std::slice::from_ref(&root))
        .expect("persist watched root");
    drop(store);
    let socket_path = runtime_directory.join("daemon.sock");
    let options = UnixDaemonRuntimeOptions {
        state_database_path: database_path,
        lease_path: runtime_directory.join("daemon.lock"),
        socket_path: socket_path.clone(),
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        scheduler_options: DiscoverySchedulerOptions::new(
            Duration::from_millis(250),
            Duration::from_secs(2),
            Duration::from_secs(30),
        )
        .expect("scheduler options"),
        idle_poll_interval: Duration::from_millis(10),
    };
    let now = Instant::now();
    let mut runtime = UnixDaemonRuntime::new(options.clone(), now).expect("singleton runtime");
    let competing_error = match UnixDaemonRuntime::new(options, now) {
        Ok(_) => panic!("a second singleton runtime must not start"),
        Err(error) => error,
    };
    assert!(
        competing_error
            .to_string()
            .contains("another Stackctl daemon owns")
    );
    assert!(socket_path.exists());
    let request = IpcRequest::new("ping-runtime", IpcPayload::Ping);
    let mut client = UnixStream::connect(&socket_path).expect("connect IPC client");
    client
        .write_all(&encode_frame(&request).expect("encode request"))
        .expect("write request");

    let iteration = runtime
        .run_iteration(now, 10_000)
        .expect("daemon iteration");

    let mut response_frame = Vec::new();
    BufReader::new(client)
        .read_until(b'\n', &mut response_frame)
        .expect("read response");
    assert_eq!(
        decode_response_frame(&response_frame).expect("decode response"),
        IpcResponse::success("ping-runtime", IpcResult::Pong)
    );
    assert_eq!(iteration.scan_reason(), Some(DiscoveryScanReason::Initial));
    assert!(
        iteration
            .reconciliation()
            .expect("initial reconciliation")
            .was_applied()
    );
    assert_eq!(iteration.request(), Some(&request));

    drop(runtime);
    std::fs::remove_dir_all(&root).expect("remove runtime fixture");
}

#[cfg(unix)]
#[test]
fn singleton_unix_runtime_reconciles_after_a_watched_root_change() {
    use super::{UnixDaemonRuntime, UnixDaemonRuntimeOptions};

    let fixture = std::env::temp_dir().join(format!(
        "s8w-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    let root = fixture.join("projects");
    let runtime_directory = fixture.join("runtime");
    std::fs::create_dir_all(&root).expect("watch test root");
    std::fs::create_dir(&runtime_directory).expect("runtime directory");
    let database_path = runtime_directory.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_watched_roots(std::slice::from_ref(&root))
        .expect("persist watched root");
    drop(store);
    let options = UnixDaemonRuntimeOptions {
        state_database_path: database_path.clone(),
        lease_path: runtime_directory.join("daemon.lock"),
        socket_path: runtime_directory.join("daemon.sock"),
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        scheduler_options: DiscoverySchedulerOptions::new(
            Duration::from_millis(25),
            Duration::from_millis(250),
            Duration::from_secs(60),
        )
        .expect("scheduler options"),
        idle_poll_interval: Duration::from_millis(5),
    };
    let started_at = Instant::now();
    let mut runtime = UnixDaemonRuntime::new(options, started_at).expect("singleton runtime");
    runtime
        .run_iteration(started_at, 10_000)
        .expect("initial daemon iteration");
    let project = root.join("bill");
    std::fs::create_dir(&project).expect("project directory");
    std::fs::write(
        project.join(".stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("project config");

    let deadline = Instant::now() + Duration::from_secs(2);
    let iteration = loop {
        let now = Instant::now();
        let iteration = runtime
            .run_iteration(now, 10_001)
            .expect("event-driven daemon iteration");
        if iteration.scan_reason() == Some(DiscoveryScanReason::FilesystemEvents) {
            break iteration;
        }
        assert!(
            now < deadline,
            "filesystem event did not trigger reconciliation"
        );
        std::thread::sleep(Duration::from_millis(5));
    };

    assert!(
        iteration
            .reconciliation()
            .expect("filesystem reconciliation")
            .was_applied()
    );
    drop(runtime);
    let store = SqliteStateStore::open(&database_path).expect("reopen state store");
    let projects = store.projects().expect("load registered projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].project_name(), "bill");
    drop(store);
    std::fs::remove_dir_all(&fixture).expect("remove runtime fixture");
}

#[cfg(unix)]
#[test]
fn singleton_unix_runtime_never_deletes_a_non_socket_endpoint() {
    use super::{UnixDaemonRuntime, UnixDaemonRuntimeOptions};

    let root = std::env::temp_dir().join(format!(
        "s8f-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("runtime test root");
    let socket_path = root.join("daemon.sock");
    std::fs::write(&socket_path, "must-survive").expect("endpoint sentinel");
    let options = UnixDaemonRuntimeOptions {
        state_database_path: root.join("state.sqlite3"),
        lease_path: root.join("daemon.lock"),
        socket_path: socket_path.clone(),
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        scheduler_options: DiscoverySchedulerOptions::new(
            Duration::from_millis(250),
            Duration::from_secs(2),
            Duration::from_secs(30),
        )
        .expect("scheduler options"),
        idle_poll_interval: Duration::from_millis(10),
    };

    let error = match UnixDaemonRuntime::new(options, Instant::now()) {
        Ok(_) => panic!("regular endpoint file must be preserved"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("is not an owned Unix socket"));
    assert_eq!(
        std::fs::read_to_string(&socket_path).expect("read endpoint sentinel"),
        "must-survive"
    );

    std::fs::remove_dir_all(&root).expect("remove runtime fixture");
}

#[cfg(unix)]
#[test]
fn watched_root_scan_never_follows_a_symlinked_project_config() {
    use std::os::unix::fs::symlink;

    let root = temporary_directory("symlink-config");
    let target = root.join("outside.yaml");
    std::fs::write(
        &target,
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("symlink target");
    symlink(&target, root.join(".stackctl.yaml")).expect("config symlink");

    let report = discover_project_sources(
        std::slice::from_ref(&root),
        ProjectDiscoveryOptions::bounded_defaults(),
    )
    .expect("bounded discovery");

    assert!(report.sources().is_empty());
    assert_eq!(report.issues().len(), 1);
    assert!(report.issues()[0].to_string().contains("symbolic link"));

    std::fs::remove_dir_all(&root).expect("remove symlink fixture");
}

#[test]
fn watched_root_scan_is_bounded_before_reading_oversized_configuration() {
    let root = temporary_directory("oversized-config");
    std::fs::write(root.join(".stackctl.yaml"), vec![b'x'; 129]).expect("oversized config");
    let options = ProjectDiscoveryOptions::new(8, 100, 128).expect("discovery options");

    let report =
        discover_project_sources(std::slice::from_ref(&root), options).expect("bounded discovery");

    assert!(report.sources().is_empty());
    assert!(
        report.issues()[0]
            .to_string()
            .contains("is 129 bytes; maximum is 128 bytes")
    );

    std::fs::remove_dir_all(&root).expect("remove oversized fixture");
}

fn temporary_lock_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "stackctl-v8-{name}-{}-{unique}.lock",
        std::process::id()
    ))
}

fn temporary_directory(name: &str) -> PathBuf {
    let path = temporary_lock_path(name).with_extension("directory");
    std::fs::create_dir(&path).expect("temporary directory");
    path
}

fn remove_lock(lock_path: &Path) {
    if lock_path.exists() {
        std::fs::remove_file(lock_path).expect("remove singleton lease");
    }
}
