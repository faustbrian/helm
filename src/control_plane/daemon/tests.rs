use super::{
    DaemonRequestDispatchOptions, DiscoveryScanReason, DiscoveryScheduler,
    DiscoverySchedulerOptions, EngineConnectionFuture, EngineConnectionOutcome,
    EngineConnectionSupervisor, EngineConnector, EngineReconciliationPlanOptions, IpcEventJournal,
    ProjectCommandQueue, ProjectDiscoveryOptions, ProjectLogBuffer, ProjectLogRequest,
    ProjectLogSessionRegistry, ProjectLogTarget, QueuedProjectCommand, ResourceHealthRegistry,
    RetryBackoff, RetryBackoffOptions, SingletonLease, discover_project_sources,
    dispatch_daemon_request, execute_project_logs, execute_queued_project_command,
    invalidate_engine_connection, plan_engine_reconciliation, publish_project_command_result,
    reconcile_watched_roots, requires_followup_reconciliation, restore_project_command_operations,
};
use crate::control_plane::application::{ControlPlane, ProjectSource, plan_project_registry};
use crate::control_plane::daemon::ipc::{
    IpcEventKind, IpcLogSessionState, IpcManagedEnvironment, IpcOutputStream, IpcPayload,
    IpcProjectCommand, IpcProjectStatus, IpcRequest, IpcResourceHealth, IpcResourceLifecycle,
    IpcResourceStatus, IpcResponse, IpcResult,
};
use crate::control_plane::engine::ContainerHealth;
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::resolve_execution_plan;
use crate::control_plane::state::{
    DaemonOperationRecord, DaemonOperationRecordOptions, DaemonOperationStatus,
    DaemonOperationTransitionOptions, EnvironmentLifecycle, ManagedEnvironmentRecord,
    ManagedEnvironmentRecordOptions, ProjectRecord, ResourceLifecycle, ResourceRecord,
    ResourceRecordOptions, ResourceRetention, SqliteStateStore, StateStore,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn project_logs_stream_from_exact_live_owned_containers() {
    let engine = RecordingProjectCommandEngine::new(vec![observed_project_application(
        "container-app",
        "install-1",
        "bill",
        "app",
    )]);
    let request = ProjectLogRequest::new(
        "logs-42".to_owned(),
        "bill".to_owned(),
        vec![ProjectLogTarget::new(
            "app".to_owned(),
            "container-app".to_owned(),
            Some("bill".to_owned()),
        )],
        false,
        Some(100),
    );
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime
        .block_on(execute_project_logs(
            engine,
            request,
            "install-1".to_owned(),
            8,
            sender,
        ))
        .expect("project logs");

    let first = receiver.try_recv().expect("stdout log chunk");
    assert_eq!(first.service(), "app");
    assert_eq!(first.stream(), IpcOutputStream::Stdout);
    assert_eq!(first.bytes(), b"ready\n");
}

#[test]
fn project_log_buffer_is_bounded_and_fails_loudly_for_expired_cursors() {
    let mut buffer = ProjectLogBuffer::new(2).expect("log buffer");
    buffer.append("app", IpcOutputStream::Stdout, b"one");
    buffer.append("app", IpcOutputStream::Stderr, b"two");
    buffer.append("db", IpcOutputStream::Stdout, b"three");

    let error = buffer.poll(Some(0), 10).expect_err("expired cursor");
    assert!(error.to_string().contains("expired"));

    let (chunks, cursor, state) = buffer.poll(Some(1), 10).expect("retained page");
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].sequence(), 2);
    assert_eq!(chunks[1].service(), "db");
    assert_eq!(cursor, 3);
    assert_eq!(state, IpcLogSessionState::Streaming);
}

#[test]
fn project_log_buffer_pages_without_skipping_and_exposes_terminal_state() {
    let mut buffer = ProjectLogBuffer::new(4).expect("log buffer");
    buffer.append("app", IpcOutputStream::Stdout, b"one");
    buffer.append("app", IpcOutputStream::Stdout, b"two");
    buffer.complete();

    let (first, first_cursor, state) = buffer.poll(None, 1).expect("first page");
    assert_eq!(first.len(), 1);
    assert_eq!(first_cursor, 1);
    assert_eq!(state, IpcLogSessionState::Streaming);

    let (second, second_cursor, state) = buffer.poll(Some(first_cursor), 1).expect("second page");
    assert_eq!(second.len(), 1);
    assert_eq!(second_cursor, 2);
    assert!(state.terminal());
}

#[test]
fn cancelled_project_log_sessions_release_capacity_immediately() {
    let mut sessions = ProjectLogSessionRegistry::new(1, 4).expect("log sessions");
    sessions
        .open(project_log_request("logs-1"))
        .expect("first session");

    sessions.cancel("logs-1").expect("cancel session");
    sessions
        .open(project_log_request("logs-2"))
        .expect("replacement session");

    assert!(sessions.should_stop("logs-1"));
    assert!(!sessions.should_stop("logs-2"));
}

#[test]
fn idle_project_log_sessions_expire_and_release_capacity() {
    let started_at = Instant::now();
    let mut sessions = ProjectLogSessionRegistry::with_idle_timeout(1, 4, Duration::from_secs(30))
        .expect("log sessions");
    sessions
        .open_at(project_log_request("logs-1"), started_at)
        .expect("first session");

    let expired = sessions.expire_idle(started_at + Duration::from_secs(30));
    sessions
        .open_at(
            project_log_request("logs-2"),
            started_at + Duration::from_secs(30),
        )
        .expect("replacement session");

    assert_eq!(expired, vec!["logs-1"]);
    assert!(sessions.should_stop("logs-1"));
}

#[test]
fn queued_project_commands_execute_only_in_the_exact_owned_application() {
    let engine = RecordingProjectCommandEngine::new(vec![observed_project_application(
        "container-app",
        "install-1",
        "bill",
        "app",
    )]);
    let operation = queued_composer_command("operation-42", "bill", "app");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("async runtime");

    let result = runtime.block_on(execute_queued_project_command(
        engine.clone(),
        operation,
        "install-1".to_owned(),
        8,
    ));

    let output = result.outcome().as_ref().expect("command output");
    assert_eq!(result.operation_id(), "operation-42");
    assert_eq!(output.stdout(), b"installed\n");
    assert_eq!(output.stderr(), b"notice\n");
    assert_eq!(engine.started(), 1);
    assert_eq!(engine.containers(), ["container-app"]);
}

#[test]
fn queued_project_commands_reject_ambiguous_or_unowned_applications_before_exec() {
    let exact = observed_project_application("app-1", "install-1", "bill", "app");
    let malformed = malformed_project_application("malformed", "install-1", "bill", "app");
    let cases = [
        (Vec::new(), "is not ready"),
        (vec![exact.clone(), exact], "duplicate owned containers"),
        (
            vec![observed_project_application(
                "foreign",
                "install-2",
                "bill",
                "app",
            )],
            "is not ready",
        ),
        (vec![malformed], "has invalid ownership"),
    ];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("async runtime");

    for (observed, expected) in cases {
        let engine = RecordingProjectCommandEngine::new(observed);
        let result = runtime.block_on(execute_queued_project_command(
            engine.clone(),
            queued_composer_command("operation-42", "bill", "app"),
            "install-1".to_owned(),
            8,
        ));

        let error = result.outcome().as_ref().expect_err("rejected command");
        assert!(
            error.to_string().contains(expected),
            "unexpected error: {error}"
        );
        assert_eq!(engine.started(), 0);
    }
}

#[test]
fn project_command_results_publish_binary_safe_output_before_completion() {
    use crate::control_plane::daemon::ipc::IpcOutputStream;
    use base64::Engine as _;

    let root = temporary_directory("project-command-result");
    let store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let mut control_plane = ControlPlane::new(store);
    let mut journal = IpcEventJournal::default();
    let engine = RecordingProjectCommandEngine::new(vec![observed_project_application(
        "container-app",
        "install-1",
        "bill",
        "app",
    )]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("async runtime");
    let queued = queued_composer_command("operation-42", "bill", "app");
    let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
        operation_id: "operation-42".to_owned(),
        kind: "project_command".to_owned(),
        payload_json: queued.payload_json().expect("durable command payload"),
        status: DaemonOperationStatus::Queued,
        created_at_unix_seconds: 100,
        updated_at_unix_seconds: 100,
    });
    let accepted_json = serde_json::to_string(&IpcEventKind::Accepted).expect("accepted event");
    let accepted = control_plane
        .enqueue_daemon_operation(&operation, &accepted_json, journal.capacity())
        .expect("durable queued operation");
    journal
        .append_record(accepted)
        .expect("accepted event record");
    drop(
        control_plane
            .transition_daemon_operation(DaemonOperationTransitionOptions {
                operation_id: "operation-42",
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Running,
                updated_at_unix_seconds: 101,
                event_kind_json: None,
                event_retention_limit: journal.capacity(),
            })
            .expect("running operation"),
    );
    let result = runtime.block_on(execute_queued_project_command(
        engine,
        queued,
        "install-1".to_owned(),
        8,
    ));

    publish_project_command_result(&mut control_plane, &mut journal, result, 102)
        .expect("publish command result");

    let events = journal.events_after(Some(0)).expect("command events");
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].kind(), &IpcEventKind::Accepted);
    assert_eq!(
        events[1].kind(),
        &IpcEventKind::Output {
            stream: IpcOutputStream::Stdout,
            data_base64: base64::engine::general_purpose::STANDARD.encode(b"installed\n"),
        }
    );
    assert_eq!(
        events[2].kind(),
        &IpcEventKind::Output {
            stream: IpcOutputStream::Stderr,
            data_base64: base64::engine::general_purpose::STANDARD.encode(b"notice\n"),
        }
    );
    assert_eq!(events[3].kind(), &IpcEventKind::Completed);

    drop(control_plane);
    std::fs::remove_dir_all(root).expect("remove command result fixture");
}

#[test]
fn daemon_restart_restores_queued_commands_and_fails_ambiguous_running_commands() {
    let root = temporary_directory("project-command-restart");
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let accepted_json = serde_json::to_string(&IpcEventKind::Accepted).expect("accepted event");
    for (operation_id, created_at) in [("queued-42", 100), ("running-42", 101)] {
        let queued = queued_composer_command(operation_id, "bill", "app");
        let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
            operation_id: operation_id.to_owned(),
            kind: "project_command".to_owned(),
            payload_json: queued.payload_json().expect("durable command payload"),
            status: DaemonOperationStatus::Queued,
            created_at_unix_seconds: created_at,
            updated_at_unix_seconds: created_at,
        });
        store
            .enqueue_daemon_operation(&operation, &accepted_json, 256)
            .expect("persist queued command");
    }
    drop(
        store
            .transition_daemon_operation(DaemonOperationTransitionOptions {
                operation_id: "running-42",
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Running,
                updated_at_unix_seconds: 102,
                event_kind_json: None,
                event_retention_limit: 256,
            })
            .expect("claim running command"),
    );

    let mut restored =
        restore_project_command_operations(&mut store, 200).expect("restore project commands");

    assert_eq!(restored.len(), 1);
    let queued = restored.pop_front().expect("restored queued command");
    assert_eq!(queued.operation_id(), "queued-42");
    assert_eq!(queued.plan().arguments(), ["composer", "install"]);
    let active = store
        .active_daemon_operations()
        .expect("active daemon operations");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].operation_id(), "queued-42");
    let events = store.daemon_events().expect("daemon events");
    let interrupted = events.last().expect("interrupted terminal event");
    assert_eq!(interrupted.operation_id(), "running-42");
    assert!(
        interrupted
            .kind_json()
            .contains("project_command_interrupted")
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove restart fixture");
}

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
    assert!(supervisor.is_connected());

    let disconnected_at = started_at + retry.duration();
    let mut resource_health = ResourceHealthRegistry::default();
    resource_health
        .record("container-app", ContainerHealth::Healthy, 10_000)
        .expect("valid health observation");
    let reconnect =
        invalidate_engine_connection(&mut supervisor, &mut resource_health, disconnected_at);
    assert!(!supervisor.is_connected());
    assert_eq!(resource_health.observation("container-app"), None);
    assert!(matches!(
        runtime.block_on(supervisor.poll(disconnected_at + reconnect.duration() / 2)),
        EngineConnectionOutcome::BackingOff { .. }
    ));
}

#[test]
fn complete_engine_plans_include_exact_applications_and_gateway_routes() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        concat!(
            "schema_version: 8\nproject: bill\nservices:\n  app:\n",
            "    image: ghcr.io/acme/bill@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        )
        .to_owned(),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let plan = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &[],
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &[],
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect("complete Engine plan");

    assert_eq!(plan.applications().len(), 1);
    assert_eq!(plan.applications()[0].request().name(), "stackctl-bill-app");
    assert_eq!(plan.gateway().routes().len(), 1);
    assert_eq!(
        plan.gateway().routes()[0].domain(),
        "bill-app.stackctl.localhost"
    );
}

#[test]
fn complete_engine_plans_include_dedicated_project_services_without_routes() {
    let image = concat!(
        "memcached@sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  cache:\n    preset: memcached\n    version: '1'\n    image: {image}\n    command: [memcached, -m, '128']\n    environment:\n      CACHE_NAMESPACE: bill\n"
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let plan = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &[],
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &[],
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect("complete Engine plan");

    assert_eq!(plan.dedicated_services().len(), 1);
    let service = &plan.dedicated_services()[0];
    assert_eq!(service.name(), "stackctl-bill-cache");
    assert_eq!(service.image(), image);
    assert_eq!(
        service.metadata().kind(),
        crate::control_plane::engine::ResourceKind::ProjectService
    );
    assert_eq!(service.metadata().project_id(), Some("bill"));
    assert_eq!(service.metadata().resource_id(), Some("cache"));
    assert_eq!(service.network(), Some("stackctl"));
    assert_eq!(service.platform(), Some("linux/arm64"));
    assert_eq!(service.command(), ["memcached", "-m", "128"]);
    assert_eq!(
        service.environment().get("CACHE_NAMESPACE"),
        Some(&"bill".to_owned())
    );
    assert!(service.port_bindings().is_empty());
    assert!(plan.gateway().routes().is_empty());
}

#[cfg(unix)]
#[test]
fn managed_environment_planning_replaces_absent_shared_values_with_empty_state() {
    use super::unix_daemon_runtime::merge_prepared_environments;

    let image = concat!(
        "ghcr.io/acme/bill@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!("schema_version: 8\nproject: bill\nservices:\n  app:\n    image: {image}\n"),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let environments = merge_prepared_environments(&execution, &[]).expect("managed environments");

    assert_eq!(environments.len(), 1);
    assert_eq!(environments[0].project_id(), "bill");
    assert!(environments[0].values().is_empty());
    assert_eq!(environments[0].lifecycle(), EnvironmentLifecycle::Active);
}

#[test]
fn complete_engine_plans_bind_project_processes_to_their_application_runtime() {
    let image = concat!(
        "ghcr.io/acme/bill@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  app:\n    image: {image}\n    environment:\n      APP_MODE: local\n  worker:\n    preset: queue-worker\n    depends_on: [app]\n    environment:\n      WORKER_MODE: steady\n"
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let plan = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &[],
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &[],
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect("complete Engine plan");

    assert_eq!(plan.applications().len(), 1);
    assert_eq!(plan.processes().len(), 1);
    assert_eq!(plan.processes()[0].name(), "stackctl-bill-worker");
    assert_eq!(plan.processes()[0].image(), image);
    assert_eq!(
        plan.processes()[0].command(),
        ["php", "artisan", "queue:work", "--no-interaction"]
    );
    assert_eq!(
        plan.processes()[0].environment().get("APP_MODE"),
        Some(&"local".to_owned())
    );
    assert_eq!(
        plan.processes()[0].environment().get("WORKER_MODE"),
        Some(&"steady".to_owned())
    );
    assert_eq!(plan.gateway().routes().len(), 1);
}

#[test]
fn project_processes_without_one_application_dependency_block_complete_planning() {
    let image = concat!(
        "ghcr.io/acme/bill@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  app:\n    image: {image}\n  worker:\n    preset: queue-worker\n"
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let error = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &[],
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &[],
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect_err("process without application dependency");

    assert_eq!(
        error.to_string(),
        "project process 'bill-worker' must depend on exactly one project application"
    );
}

#[test]
fn orphaned_project_workload_scopes_require_adoption_before_engine_planning() {
    let image = concat!(
        "ghcr.io/acme/bill@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!("schema_version: 8\nproject: bill\nservices:\n  app:\n    image: {image}\n"),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let orphan = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "old-bill-app".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:old-runtime".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:old-desired".to_owned(),
        retention: ResourceRetention::Disposable,
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    })
    .with_scope_id("app");
    let retained = vec![orphan];

    let error = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &[],
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &retained,
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect_err("orphaned application requires adoption");

    assert_eq!(
        error.to_string(),
        "project workload 'bill-app' is orphaned; run explicit project adoption before reconciliation"
    );

    let active = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "current-bill-app".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:current-runtime".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:current-desired".to_owned(),
        retention: ResourceRetention::Disposable,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("app");
    let resources = vec![retained[0].clone(), active];
    let plan = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &[],
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &resources,
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect("active scope permits retained replacement history");

    assert_eq!(plan.applications().len(), 1);
}

#[test]
fn complete_engine_plans_include_prepared_attributed_shared_routes() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        concat!(
            "schema_version: 8\nproject: bill\nservices:\n  mailpit:\n",
            "    preset: mailpit\n    version: \"1\"\n",
            "    image: axllent/mailpit@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        )
        .to_owned(),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let prepared = vec![("bill".to_owned(), "mailpit".to_owned())];
    let shared_routes = vec![
        GatewayRoute::new(
            "bill-mailpit.stackctl.localhost",
            "http://stackctl-shared-mailpit:8025",
        )
        .expect("shared route"),
    ];

    let plan = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &prepared,
        shared_routes: &shared_routes,
        managed_environments: &[],
        durable_resources: &[],
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect("complete Engine plan");

    assert!(plan.applications().is_empty());
    assert_eq!(plan.gateway().routes(), shared_routes);
}

#[test]
fn unsupported_strategies_block_complete_engine_planning_before_mutation() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        concat!(
            "schema_version: 8\nproject: bill\nservices:\n",
            "  db:\n    preset: postgres\n",
            "  app:\n    image: ghcr.io/acme/bill@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            "    depends_on: [db]\n"
        )
        .to_owned(),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let error = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &[],
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &[],
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect_err("unsupported shared service");

    assert_eq!(
        error.to_string(),
        "service 'bill-db' strategy SharedByCompatibility has no registered Engine reconciler"
    );

    let prepared = vec![("bill".to_owned(), "db".to_owned())];
    let plan = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &prepared,
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &[],
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect("prepared shared service");

    assert_eq!(plan.applications().len(), 1);
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
    use super::EngineReconciliationSchedule;

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
    let mut engine_schedule = EngineReconciliationSchedule::default();
    engine_schedule.observe(&applied).expect("resolve registry");
    assert!(engine_schedule.may_reconcile());
    assert!(engine_schedule.is_due());
    assert_eq!(
        engine_schedule
            .desired_registry()
            .expect("last complete registry")
            .projects()[0]
            .project_name(),
        "bill"
    );
    assert_eq!(
        engine_schedule
            .execution_plan()
            .expect("resolved execution plan")
            .services()
            .iter()
            .map(|service| service.strategy())
            .collect::<Vec<_>>(),
        vec![crate::control_plane::ServiceDeploymentStrategy::ProjectApplication]
    );
    engine_schedule.complete();
    assert!(!engine_schedule.is_due());
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
    engine_schedule.observe(&blocked).expect("block registry");
    assert!(!engine_schedule.may_reconcile());
    assert!(!engine_schedule.is_due());
    assert_eq!(
        engine_schedule
            .desired_registry()
            .expect("retained complete registry")
            .projects()[0]
            .project_name(),
        "bill"
    );
    assert_eq!(
        engine_schedule
            .execution_plan()
            .expect("retained execution plan")
            .services()
            .iter()
            .map(|service| service.strategy())
            .collect::<Vec<_>>(),
        vec![crate::control_plane::ServiceDeploymentStrategy::ProjectApplication]
    );
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
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        now_unix_seconds: 10_000,
    });

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

    let subscription = IpcRequest::new(
        "events-42",
        IpcPayload::SubscribeEvents {
            after_sequence: Some(0),
        },
    );
    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &subscription,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        now_unix_seconds: 10_001,
    });
    let crate::control_plane::daemon::ipc::IpcOutcome::Success {
        result: IpcResult::Events {
            events,
            latest_sequence,
        },
    } = response.outcome()
    else {
        panic!("event subscription should succeed");
    };
    assert_eq!(*latest_sequence, 2);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence(), 1);
    assert_eq!(events[0].operation_id(), "reconcile-42");
    assert_eq!(events[0].kind(), &IpcEventKind::Accepted);
    assert_eq!(events[1].sequence(), 2);
    assert_eq!(events[1].operation_id(), "reconcile-42");
    assert_eq!(events[1].kind(), &IpcEventKind::Completed);

    drop(control_plane);
    let store = SqliteStateStore::open(&database_path).expect("reopen daemon state");
    let persisted_events = store.daemon_events().expect("load persisted daemon events");
    assert_eq!(
        persisted_events
            .iter()
            .map(crate::control_plane::state::DaemonEventRecord::sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    drop(store);
    std::fs::remove_dir_all(&root).expect("remove reconciliation fixture");
}

#[test]
fn daemon_project_status_reports_durable_runtime_and_logical_ownership() {
    let root = temporary_directory("ipc-project-status");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(
        project_path.clone(),
        "bill".to_owned(),
        vec!["bill-app.stackctl.localhost".to_owned()],
    );
    let application = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "container-app".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:application".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("app");
    let logical = crate::control_plane::state::LogicalResourceRecord::new(
        crate::control_plane::state::LogicalResourceRecordOptions {
            logical_resource_id: "bill/database".to_owned(),
            shared_resource_id: "postgres-17".to_owned(),
            project_id: "bill".to_owned(),
            service_id: "db".to_owned(),
            kind: "postgresql".to_owned(),
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            desired_revision: "sha256:desired".to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        },
    );
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    store.replace_project(&project).expect("register project");
    store
        .upsert_resources(std::slice::from_ref(&application))
        .expect("persist application");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "status-42",
        IpcPayload::ProjectStatus {
            canonical_path: project_path,
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();
    let mut resource_health = ResourceHealthRegistry::default();
    resource_health
        .record("container-app", ContainerHealth::Healthy, 9_998)
        .expect("record application health");
    resource_health
        .record("postgres-17", ContainerHealth::Starting, 9_800)
        .expect("record shared health");

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_logs: &mut project_logs,
        resource_health: &resource_health,
        now_unix_seconds: 10_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "status-42",
            IpcResult::ProjectStatus {
                project: IpcProjectStatus::new(
                    "bill".to_owned(),
                    vec!["bill-app.stackctl.localhost".to_owned()],
                    vec![
                        IpcResourceStatus::new(
                            "app".to_owned(),
                            "project_application".to_owned(),
                            IpcResourceLifecycle::Active,
                            IpcResourceHealth::Healthy,
                            Some(9_998),
                            false,
                        ),
                        IpcResourceStatus::new(
                            "db".to_owned(),
                            "postgresql".to_owned(),
                            IpcResourceLifecycle::Active,
                            IpcResourceHealth::Unknown,
                            Some(9_800),
                            true,
                        ),
                    ],
                ),
            },
        )
    );

    drop(control_plane);
    std::fs::remove_dir_all(root).expect("remove status fixture");
}

#[test]
fn daemon_project_logs_resolve_exact_owned_services_before_opening_a_session() {
    let root = temporary_directory("ipc-project-logs");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let application = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "container-app".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:application".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("app");
    let shared_database = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "postgres-17".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let logical_database = crate::control_plane::state::LogicalResourceRecord::new(
        crate::control_plane::state::LogicalResourceRecordOptions {
            logical_resource_id: "bill/database".to_owned(),
            shared_resource_id: "postgres-17".to_owned(),
            project_id: "bill".to_owned(),
            service_id: "db".to_owned(),
            kind: "postgresql".to_owned(),
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            desired_revision: "sha256:desired".to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        },
    );
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    store.replace_project(&project).expect("register project");
    store
        .upsert_resources(&[application, shared_database])
        .expect("persist containers");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical_database))
        .expect("persist logical database");
    let mut control_plane = ControlPlane::new(store);
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();
    let open = IpcRequest::new(
        "logs-42",
        IpcPayload::OpenProjectLogs {
            canonical_path: project_path,
            services: vec!["app".to_owned(), "db".to_owned()],
            follow: true,
            tail: Some(100),
        },
    );

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &open,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        now_unix_seconds: 10_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "logs-42",
            IpcResult::Accepted {
                operation_id: "logs-42".to_owned(),
            },
        )
    );
    let pending = project_logs.take_pending().expect("pending log session");
    assert_eq!(pending.project_id(), "bill");
    assert_eq!(pending.targets()[0].service(), "app");
    assert_eq!(pending.targets()[0].resource_id(), "container-app");
    assert_eq!(pending.targets()[0].project_id(), Some("bill"));
    assert_eq!(pending.targets()[1].service(), "db");
    assert_eq!(pending.targets()[1].resource_id(), "postgres-17");
    assert_eq!(pending.targets()[1].project_id(), None);

    let poll = IpcRequest::new(
        "logs-poll-42",
        IpcPayload::PollProjectLogs {
            session_id: "logs-42".to_owned(),
            after_sequence: None,
            max_chunks: 64,
        },
    );
    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &poll,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        now_unix_seconds: 10_001,
    });
    assert_eq!(
        response,
        IpcResponse::success(
            "logs-poll-42",
            IpcResult::ProjectLogs {
                session_id: "logs-42".to_owned(),
                chunks: Vec::new(),
                latest_sequence: 0,
                state: IpcLogSessionState::Starting,
            },
        )
    );

    drop(control_plane);
    std::fs::remove_dir_all(root).expect("remove log fixture");
}

#[test]
fn daemon_project_environment_returns_only_the_exact_active_managed_values() {
    let root = temporary_directory("ipc-project-environment");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment".to_owned(),
        values: BTreeMap::from([
            ("DB_HOST".to_owned(), "postgres.internal".to_owned()),
            ("DB_PASSWORD".to_owned(), "secret-value".to_owned()),
        ]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    store.replace_project(&project).expect("register project");
    store
        .replace_managed_environment(&environment)
        .expect("persist environment");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "environment-42",
        IpcPayload::ProjectEnvironment {
            canonical_path: project_path,
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        now_unix_seconds: 10_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "environment-42",
            IpcResult::ProjectEnvironment {
                environment: IpcManagedEnvironment::new(
                    "bill".to_owned(),
                    "sha256:environment".to_owned(),
                    BTreeMap::from([
                        ("DB_HOST".to_owned(), "postgres.internal".to_owned()),
                        ("DB_PASSWORD".to_owned(), "secret-value".to_owned()),
                    ]),
                ),
            },
        )
    );
    assert!(!format!("{response:?}").contains("secret-value"));

    drop(control_plane);
    std::fs::remove_dir_all(root).expect("remove environment fixture");
}

#[test]
fn daemon_adoption_request_reactivates_the_exact_registered_project() {
    let root = temporary_directory("ipc-adoption");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(
        project_path.clone(),
        "bill".to_owned(),
        vec!["bill-app.stackctl.localhost".to_owned()],
    );
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment".to_owned(),
        values: BTreeMap::new(),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("register project");
    store
        .replace_managed_environment(&environment)
        .expect("persist environment");
    store
        .orphan_project(&project_path, 12_345)
        .expect("orphan project state");
    store
        .replace_project(&project)
        .expect("register adoption target");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "adopt-42",
        IpcPayload::AdoptProject {
            canonical_path: project_path,
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        now_unix_seconds: 20_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "adopt-42",
            IpcResult::ProjectAdopted {
                project_id: "bill".to_owned(),
            },
        )
    );

    drop(control_plane);
    std::fs::remove_dir_all(&root).expect("remove adoption fixture");
}

#[test]
fn daemon_project_command_request_queues_an_exact_registered_runtime() {
    let root = temporary_directory("ipc-project-command");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(
        project_path.clone(),
        "bill".to_owned(),
        vec!["bill-app.stackctl.localhost".to_owned()],
    );
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment".to_owned(),
        values: BTreeMap::from([("DB_HOST".to_owned(), "postgres.internal".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("register project");
    store
        .replace_managed_environment(&environment)
        .expect("persist managed environment");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "command-42",
        IpcPayload::RunProjectCommand {
            canonical_path: project_path,
            service: "app".to_owned(),
            command: IpcProjectCommand::Composer {
                arguments: vec!["install".to_owned(), "--no-interaction".to_owned()],
            },
            timeout_seconds: 300,
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        now_unix_seconds: 30_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "command-42",
            IpcResult::Accepted {
                operation_id: "command-42".to_owned(),
            },
        )
    );
    assert_eq!(project_commands.len(), 1);
    let queued = project_commands
        .pop_front()
        .expect("queued project command");
    assert_eq!(queued.operation_id(), "command-42");
    assert_eq!(queued.service_id(), "app");
    assert_eq!(queued.plan().project_id(), "bill");
    assert_eq!(
        queued.plan().arguments(),
        ["composer", "install", "--no-interaction"]
    );
    assert_eq!(event_journal.latest_sequence(), 1);

    drop(control_plane);
    std::fs::remove_dir_all(&root).expect("remove command fixture");
}

#[test]
fn mutating_daemon_requests_schedule_a_complete_followup_scan() {
    let adopt = IpcRequest::new(
        "adopt-42",
        IpcPayload::AdoptProject {
            canonical_path: PathBuf::from("/work/bill"),
        },
    );

    assert!(requires_followup_reconciliation(&adopt));
    assert!(requires_followup_reconciliation(&IpcRequest::new(
        "reconcile-42",
        IpcPayload::Reconcile,
    )));
    assert!(!requires_followup_reconciliation(&IpcRequest::new(
        "ping-42",
        IpcPayload::Ping,
    )));
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

#[derive(Clone)]
struct RecordingProjectCommandEngine {
    observed: Vec<crate::control_plane::engine::ObservedContainer>,
    execution: std::sync::Arc<RecordingProjectCommandExecution>,
}

#[derive(Default)]
struct RecordingProjectCommandExecution {
    started: std::sync::atomic::AtomicUsize,
    containers: std::sync::Mutex<Vec<String>>,
}

impl RecordingProjectCommandEngine {
    fn new(observed: Vec<crate::control_plane::engine::ObservedContainer>) -> Self {
        Self {
            observed,
            execution: std::sync::Arc::new(RecordingProjectCommandExecution::default()),
        }
    }

    fn started(&self) -> usize {
        self.execution
            .started
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn containers(&self) -> Vec<String> {
        self.execution
            .containers
            .lock()
            .expect("executed containers")
            .clone()
    }
}

impl crate::control_plane::engine::ContainerDiscovery for RecordingProjectCommandEngine {
    fn discover_managed(
        &self,
    ) -> crate::control_plane::engine::EngineFuture<
        '_,
        Vec<crate::control_plane::engine::ObservedContainer>,
    > {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl crate::control_plane::engine::LogSource for RecordingProjectCommandEngine {
    fn logs<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
        _options: &'operation crate::control_plane::engine::ContainerLogOptions,
    ) -> crate::control_plane::engine::EngineFuture<
        'operation,
        crate::control_plane::engine::ContainerLogStream<'operation>,
    > {
        Box::pin(async {
            let stream: crate::control_plane::engine::ContainerLogStream<'operation> =
                Box::pin(futures_util::stream::once(async {
                    Ok(crate::control_plane::engine::LogChunk::stdout(
                        b"ready\n".to_vec(),
                    ))
                }));

            Ok(stream)
        })
    }
}

impl crate::control_plane::engine::CommandExecutor for RecordingProjectCommandEngine {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
        _request: &'operation crate::control_plane::engine::CommandRequest,
    ) -> crate::control_plane::engine::EngineFuture<
        'operation,
        crate::control_plane::engine::CommandSession,
    > {
        self.execution
            .started
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.execution
            .containers
            .lock()
            .expect("executed containers")
            .push(container.id().as_str().to_owned());
        let container_id = container.id().clone();
        Box::pin(async move {
            let (writer, _reader) = tokio::io::duplex(1024);
            let output: crate::control_plane::engine::ContainerLogStream<'static> =
                Box::pin(futures_util::stream::iter([
                    Ok(crate::control_plane::engine::LogChunk::stdout(
                        b"installed\n".to_vec(),
                    )),
                    Ok(crate::control_plane::engine::LogChunk::new(
                        crate::control_plane::engine::LogStreamKind::Stderr,
                        b"notice\n".to_vec(),
                    )),
                ]));

            Ok(crate::control_plane::engine::CommandSession::new(
                crate::control_plane::engine::CommandExecutionId::new("command-1"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation crate::control_plane::engine::CommandExecutionId,
        _container_id: &'operation crate::control_plane::engine::ContainerId,
    ) -> crate::control_plane::engine::EngineFuture<
        'operation,
        crate::control_plane::engine::CommandStatus,
    > {
        Box::pin(async { Ok(crate::control_plane::engine::CommandStatus::Exited(0)) })
    }
}

fn queued_composer_command(
    operation_id: &str,
    project_id: &str,
    service_id: &str,
) -> QueuedProjectCommand {
    let project = crate::control_plane::ProjectIdentity::resolve(
        Some(project_id),
        Path::new("/work/project"),
    )
    .expect("project identity");
    let plan = crate::control_plane::workload::ProjectCommandPlan::new(
        crate::control_plane::workload::ProjectCommandPlanOptions {
            project,
            command: crate::control_plane::workload::ProjectCommand::Composer {
                arguments: vec!["install".to_owned()],
            },
            environment: BTreeMap::new(),
            input: Vec::new(),
            timeout: Duration::from_secs(30),
        },
    )
    .expect("project command plan");

    QueuedProjectCommand::new(operation_id.to_owned(), service_id.to_owned(), plan)
}

fn project_log_request(session_id: &str) -> ProjectLogRequest {
    ProjectLogRequest::new(
        session_id.to_owned(),
        "bill".to_owned(),
        vec![ProjectLogTarget::new(
            "app".to_owned(),
            "container-app".to_owned(),
            Some("bill".to_owned()),
        )],
        true,
        None,
    )
}

fn observed_project_application(
    container_id: &str,
    installation_id: &str,
    project_id: &str,
    service_id: &str,
) -> crate::control_plane::engine::ObservedContainer {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: installation_id.to_owned(),
            kind: crate::control_plane::engine::ResourceKind::ProjectApplication,
            project_id: Some(project_id.to_owned()),
            compatibility_fingerprint: "sha256:application".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Disposable,
        },
    )
    .expect("application metadata")
    .with_resource_id(service_id)
    .expect("application service identity");

    crate::control_plane::engine::ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new(container_id),
        metadata.labels(),
    )
}

fn malformed_project_application(
    container_id: &str,
    installation_id: &str,
    project_id: &str,
    service_id: &str,
) -> crate::control_plane::engine::ObservedContainer {
    let observed =
        observed_project_application(container_id, installation_id, project_id, service_id);
    let mut labels = observed.labels().clone();
    drop(labels.remove("dev.stackctl.desired"));

    crate::control_plane::engine::ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new(container_id),
        labels,
    )
}
