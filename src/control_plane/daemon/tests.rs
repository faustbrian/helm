use super::{
    BenchmarkSnapshotProvider, DaemonRequestDispatchOptions, DiscoveryScanReason,
    DiscoveryScheduler, DiscoverySchedulerOptions, EngineConnectionFuture, EngineConnectionOutcome,
    EngineConnectionSupervisor, EngineConnector, EngineReconciliationPlanOptions,
    ImageReferenceResolution, IpcEventJournal, MigrationDecisionExecutionOptions,
    MigrationDecisionQueue, PostgresPruneExecutionOptions, PostgresPruneQueue,
    ProjectBackupExecutionOptions, ProjectBackupQueue, ProjectCommandExecutionOptions,
    ProjectCommandQueue, ProjectDiscoveryOptions, ProjectLogBuffer, ProjectLogRequest,
    ProjectLogSessionRegistry, ProjectLogTarget, ProjectRestoreExecutionOptions,
    ProjectRestoreExecutionResult, ProjectRestoreQueue, ProjectRestoreTargetPlan,
    QueuedPostgresPrune, QueuedProjectBackup, QueuedProjectCommand, QueuedProjectRestore,
    ResourceHealthRegistry, RetryBackoff, RetryBackoffOptions, ScheduledCommandClock,
    SingletonLease, collect_benchmark_snapshot, discover_project_sources, dispatch_daemon_request,
    execute_project_logs, execute_queued_migration_decision, execute_queued_postgres_prune,
    execute_queued_project_backup, execute_queued_project_command, execute_queued_project_restore,
    execute_scheduled_project_command, finalize_installation_deletion,
    invalidate_engine_connection, plan_engine_reconciliation, publish_project_command_result,
    publish_project_restore_result, queue_next_installation_deletion_prune,
    reconcile_watched_roots, requires_followup_reconciliation, restore_daemon_operation_queues,
    retry_failed_installation_deletion_prune,
};
use crate::control_plane::application::{ControlPlane, ProjectSource, plan_project_registry};
use crate::control_plane::daemon::ipc::{
    IpcBenchmarkContainerMetrics, IpcBenchmarkContainerMetricsOptions, IpcBenchmarkSnapshot,
    IpcBenchmarkTcpPort, IpcDataLifecycle, IpcEventKind, IpcLogSessionState, IpcManagedEnvironment,
    IpcMigrationDecision, IpcMigrationStatus, IpcOutcome, IpcOutputStream, IpcPayload,
    IpcProjectCommand, IpcProjectStatus, IpcRequest, IpcResourceHealth, IpcResourceLifecycle,
    IpcResourceStatus, IpcResponse, IpcResult,
};
use crate::control_plane::engine::ContainerHealth;
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::migration::{
    MigrationExecutionResult, MongoDbRestoreOptions, MongoDbVerifyTargetOptions,
    MySqlRestoreOptions, SqlServerRestoreOptions, SqlServerVerifyTargetOptions,
    restore_mongodb_database, restore_mysql_database, restore_sql_server_database,
    verify_mongodb_target, verify_sql_server_target,
};
use crate::control_plane::resolve_execution_plan;
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialGenerationError, resolve_execution_shared_instances,
};
use crate::control_plane::state::{
    DaemonOperationRecord, DaemonOperationRecordOptions, DaemonOperationStatus,
    DaemonOperationTransitionOptions, EnvironmentLifecycle, ManagedEnvironmentRecord,
    ManagedEnvironmentRecordOptions, MigrationPhase, MigrationRecord, MigrationRecordOptions,
    ProjectRecord, RecoveryPointRecord, RecoveryPointRecordOptions, ResourceLifecycle,
    ResourceRecord, ResourceRecordOptions, ResourceRetention, SqliteStateStore, StateStore,
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
    let mut options = command_execution_options(operation);
    options.managed_environment = Ok(BTreeMap::from([(
        "DB_PASSWORD".to_owned(),
        "runtime-only-secret".to_owned(),
    )]));

    let result = runtime.block_on(execute_queued_project_command(engine.clone(), options));

    let output = result.outcome().as_ref().expect("command output");
    assert_eq!(result.operation_id(), "operation-42");
    assert_eq!(output.stdout(), b"installed\n");
    assert_eq!(output.stderr(), b"notice\n");
    assert_eq!(engine.started(), 1);
    assert_eq!(engine.containers(), ["container-app"]);
    assert_eq!(
        engine.command_environments()[0].get("DB_PASSWORD"),
        Some(&"runtime-only-secret".to_owned())
    );
}

#[test]
fn scheduled_project_commands_execute_only_in_the_exact_owned_application() {
    let engine = RecordingProjectCommandEngine::new(vec![observed_project_application(
        "container-app",
        "install-1",
        "bill",
        "app",
    )]);
    let plan = crate::control_plane::workload::ScheduledProjectCommandPlan::new(
        crate::control_plane::workload::ScheduledProjectCommandPlanOptions {
            project: crate::control_plane::ProjectIdentity::resolve(
                Some("bill"),
                Path::new("/work/bill"),
            )
            .expect("project identity"),
            service: crate::control_plane::ServiceIdentity::new("scheduler")
                .expect("scheduler identity"),
            application_service: crate::control_plane::ServiceIdentity::new("app")
                .expect("application identity"),
            arguments: ["php", "artisan", "schedule:run", "--no-interaction"]
                .map(str::to_owned)
                .to_vec(),
            environment: BTreeMap::from([("APP_MODE".to_owned(), "local".to_owned())]),
            timeout: Duration::from_secs(55),
        },
    )
    .expect("scheduled command plan");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("async runtime");

    let output = runtime
        .block_on(execute_scheduled_project_command(
            engine.clone(),
            plan,
            "install-1".to_owned(),
            8,
        ))
        .expect("scheduled command output");

    assert_eq!(output.stdout(), b"installed\n");
    assert_eq!(engine.containers(), ["container-app"]);
    assert_eq!(
        engine.command_arguments()[0],
        ["php", "artisan", "schedule:run", "--no-interaction"]
    );
    assert_eq!(
        engine.command_environments()[0].get("APP_MODE"),
        Some(&"local".to_owned())
    );
}

#[test]
fn scheduled_command_clock_runs_once_per_current_minute_without_replay() {
    let mut clock = ScheduledCommandClock::default();

    assert!(clock.take_due(61));
    assert!(!clock.take_due(119));
    assert!(clock.take_due(3_599));
    assert!(!clock.take_due(3_599));
    assert!(!clock.take_due(120));
    assert!(clock.take_due(180));
}

#[test]
fn browser_project_commands_create_wait_inject_and_remove_one_ephemeral_sidecar() {
    let engine = RecordingProjectCommandEngine::new(vec![observed_project_application(
        "container-app",
        "install-1",
        "bill",
        "app",
    )]);
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        concat!(
            "schema_version: 8\nproject: bill\nservices:\n  app:\n",
            "    image: ghcr.io/acme/bill@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            "  browser:\n    preset: dusk\n    version: \"4\"\n",
            "    image: selenium/standalone-chromium@sha256:",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n"
        )
        .to_owned(),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let browser = execution
        .services()
        .iter()
        .find(|service| service.service().as_str() == "browser")
        .expect("browser service");
    let browser = crate::control_plane::workload::plan_ephemeral_browser(
        crate::control_plane::workload::EphemeralBrowserOptions {
            service: browser,
            operation_id: "operation-42",
            installation_id: "install-1",
            schema_version: 8,
            platform: "linux/arm64",
            network_name: "stackctl",
        },
    )
    .expect("browser plan");
    let browser_name = browser.request().name().to_owned();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("async runtime");

    let result = runtime.block_on(execute_queued_project_command(
        engine.clone(),
        ProjectCommandExecutionOptions {
            operation: queued_browser_command("operation-42", "bill", "app"),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            ephemeral_browser: Some(Ok(browser)),
            managed_environment: Ok(BTreeMap::new()),
        },
    ));

    result.outcome().as_ref().expect("browser command output");
    assert_eq!(engine.created(), [browser_name.clone()]);
    assert_eq!(engine.lifecycle_started(), [browser_name.clone()]);
    assert_eq!(engine.stopped(), [browser_name.clone()]);
    assert_eq!(engine.removed(), [browser_name.clone()]);
    assert_eq!(engine.containers(), ["container-app"]);
    assert_eq!(
        engine.command_environments()[0].get("DUSK_DRIVER_URL"),
        Some(&format!("http://{browser_name}:4444/wd/hub"))
    );
}

#[test]
fn browser_session_intent_survives_durable_queue_round_trips() {
    let queued = queued_browser_command("operation-42", "bill", "app");
    let payload = queued.payload_json().expect("durable browser command");

    let restored = QueuedProjectCommand::from_payload_json("operation-42".to_owned(), &payload)
        .expect("restored browser command");

    assert!(restored.plan().browser_session());
    assert_eq!(restored.plan().arguments(), queued.plan().arguments());
}

#[test]
fn durable_project_commands_reject_removed_environment_payloads() {
    let queued = queued_browser_command("operation-42", "bill", "app");
    let mut payload: serde_json::Value =
        serde_json::from_str(&queued.payload_json().expect("durable browser command"))
            .expect("command JSON");
    payload.as_object_mut().expect("command object").insert(
        "environment".to_owned(),
        serde_json::json!({"TOKEN": "legacy"}),
    );

    let error =
        QueuedProjectCommand::from_payload_json("operation-42".to_owned(), &payload.to_string())
            .expect_err("removed environment field");

    assert!(error.contains("unknown field `environment`"));
}

#[test]
fn failed_browser_commands_still_remove_the_ephemeral_sidecar() {
    let engine = RecordingProjectCommandEngine::new(vec![observed_project_application(
        "container-app",
        "install-1",
        "bill",
        "app",
    )])
    .with_command_exit_code(1);
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        concat!(
            "schema_version: 8\nproject: bill\nservices:\n  browser:\n",
            "    preset: selenium\n    version: \"4\"\n",
            "    image: selenium/standalone-chromium@sha256:",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n"
        )
        .to_owned(),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let browser = crate::control_plane::workload::plan_ephemeral_browser(
        crate::control_plane::workload::EphemeralBrowserOptions {
            service: &execution.services()[0],
            operation_id: "operation-42",
            installation_id: "install-1",
            schema_version: 8,
            platform: "linux/arm64",
            network_name: "stackctl",
        },
    )
    .expect("browser plan");
    let browser_name = browser.request().name().to_owned();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("async runtime");

    let result = runtime.block_on(execute_queued_project_command(
        engine.clone(),
        ProjectCommandExecutionOptions {
            operation: queued_browser_command("operation-42", "bill", "app"),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            ephemeral_browser: Some(Ok(browser)),
            managed_environment: Ok(BTreeMap::new()),
        },
    ));

    assert!(result.outcome().is_err());
    assert_eq!(engine.stopped(), [browser_name.clone()]);
    assert_eq!(engine.removed(), [browser_name]);
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
            command_execution_options(queued_composer_command("operation-42", "bill", "app")),
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
        command_execution_options(queued),
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

    let (mut restored, backups, prunes, restores, decisions) =
        restore_daemon_operation_queues(&mut store, 200).expect("restore daemon operations");

    assert_eq!(restored.len(), 1);
    assert_eq!(backups.len(), 0);
    assert_eq!(prunes.len(), 0);
    assert_eq!(restores.len(), 0);
    assert_eq!(decisions.len(), 0);
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
fn daemon_restart_restores_queued_backups_without_replaying_running_backups() {
    let root = temporary_directory("project-backup-restart");
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let accepted_json = serde_json::to_string(&IpcEventKind::Accepted).expect("accepted event");
    for (operation_id, created_at) in [("queued-backup", 100), ("running-backup", 101)] {
        let queued = QueuedProjectBackup::new(
            operation_id.to_owned(),
            "bill".to_owned(),
            "database".to_owned(),
            "stackctl_bill_database".to_owned(),
            "postgres_database_and_role".to_owned(),
            "sha256:postgres-17".to_owned(),
        )
        .expect("backup intent");
        let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
            operation_id: operation_id.to_owned(),
            kind: "project_backup".to_owned(),
            payload_json: queued.payload_json().expect("durable backup payload"),
            status: DaemonOperationStatus::Queued,
            created_at_unix_seconds: created_at,
            updated_at_unix_seconds: created_at,
        });
        store
            .enqueue_daemon_operation(&operation, &accepted_json, 256)
            .expect("persist queued backup");
    }
    drop(
        store
            .transition_daemon_operation(DaemonOperationTransitionOptions {
                operation_id: "running-backup",
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Running,
                updated_at_unix_seconds: 102,
                event_kind_json: None,
                event_retention_limit: 256,
            })
            .expect("claim running backup"),
    );

    let (commands, mut backups, prunes, restores, decisions) =
        restore_daemon_operation_queues(&mut store, 200).expect("restore daemon operations");

    assert_eq!(commands.len(), 0);
    assert_eq!(backups.len(), 1);
    assert_eq!(prunes.len(), 0);
    assert_eq!(restores.len(), 0);
    assert_eq!(decisions.len(), 0);
    assert_eq!(
        backups.pop_front().expect("restored backup").operation_id(),
        "queued-backup"
    );
    let events = store.daemon_events().expect("daemon events");
    let interrupted = events.last().expect("interrupted terminal event");
    assert_eq!(interrupted.operation_id(), "running-backup");
    assert!(
        interrupted
            .kind_json()
            .contains("project_backup_interrupted")
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove restart fixture");
}

#[test]
fn daemon_restart_restores_only_queued_project_restores() {
    let root = temporary_directory("project-restore-restart");
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let accepted_json = serde_json::to_string(&IpcEventKind::Accepted).expect("accepted event");
    for (operation_id, created_at) in [("queued-restore", 100), ("running-restore", 101)] {
        let queued = QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
            operation_id: operation_id.to_owned(),
            recovery_point_id: "backup-42".to_owned(),
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            logical_resource_id: "stackctl_bill_database".to_owned(),
            kind: "postgres_database_and_role".to_owned(),
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        })
        .expect("restore intent");
        let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
            operation_id: operation_id.to_owned(),
            kind: "project_restore".to_owned(),
            payload_json: queued.payload_json().expect("durable restore payload"),
            status: DaemonOperationStatus::Queued,
            created_at_unix_seconds: created_at,
            updated_at_unix_seconds: created_at,
        });
        store
            .enqueue_daemon_operation(&operation, &accepted_json, 256)
            .expect("persist queued restore");
    }
    drop(
        store
            .transition_daemon_operation(DaemonOperationTransitionOptions {
                operation_id: "running-restore",
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Running,
                updated_at_unix_seconds: 102,
                event_kind_json: None,
                event_retention_limit: 256,
            })
            .expect("claim running restore"),
    );

    let (commands, backups, prunes, mut restores, decisions) =
        restore_daemon_operation_queues(&mut store, 200).expect("restore daemon operations");

    assert_eq!(commands.len(), 0);
    assert_eq!(backups.len(), 0);
    assert_eq!(prunes.len(), 0);
    assert_eq!(restores.len(), 1);
    assert_eq!(decisions.len(), 0);
    assert_eq!(
        restores
            .pop_front()
            .expect("restored project restore")
            .operation_id(),
        "queued-restore"
    );
    let events = store.daemon_events().expect("daemon events");
    let interrupted = events.last().expect("interrupted terminal event");
    assert_eq!(interrupted.operation_id(), "running-restore");
    assert!(
        interrupted
            .kind_json()
            .contains("project_restore_interrupted")
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove restore restart fixture");
}

#[test]
fn mysql_restore_intent_is_supported_and_secret_free() {
    let queued = QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
        operation_id: "restore-mysql".to_owned(),
        recovery_point_id: "backup-mysql".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        kind: "mysql_database".to_owned(),
        compatibility_fingerprint: format!("sha256:{}", "b".repeat(64)),
    })
    .expect("MySQL restore intent");

    let payload = queued.payload_json().expect("durable MySQL restore");

    assert!(payload.contains("mysql_database"));
    assert!(!payload.contains("password"));
    assert!(!payload.contains("secret"));
}

#[test]
fn project_restore_result_publishes_operator_gated_cutover_evidence() {
    use base64::Engine as _;

    let root = temporary_directory("project-restore-result");
    let store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let mut control_plane = ControlPlane::new(store);
    let mut journal = IpcEventJournal::default();
    let queued = QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
        operation_id: "restore-42".to_owned(),
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
    })
    .expect("restore intent");
    let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
        operation_id: "restore-42".to_owned(),
        kind: "project_restore".to_owned(),
        payload_json: queued.payload_json().expect("durable restore payload"),
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
                operation_id: "restore-42",
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Running,
                updated_at_unix_seconds: 101,
                event_kind_json: None,
                event_retention_limit: journal.capacity(),
            })
            .expect("running operation"),
    );

    publish_project_restore_result(
        &mut control_plane,
        &mut journal,
        ProjectRestoreExecutionResult::new(
            queued,
            Ok(MigrationExecutionResult::AwaitingConfirmation),
        ),
        102,
    )
    .expect("publish restore result");

    let events = journal.events_after(Some(0)).expect("restore events");
    assert_eq!(events.len(), 3);
    let IpcEventKind::Output { data_base64, .. } = events[1].kind() else {
        panic!("restore evidence must precede completion");
    };
    let evidence = base64::engine::general_purpose::STANDARD
        .decode(data_base64)
        .expect("decode restore evidence");
    let evidence: serde_json::Value =
        serde_json::from_slice(&evidence).expect("restore evidence JSON");
    assert_eq!(evidence["migration_id"], "restore-42");
    assert_eq!(evidence["state"], "awaiting_confirmation");
    assert_eq!(events[2].kind(), &IpcEventKind::Completed);

    drop(control_plane);
    std::fs::remove_dir_all(root).expect("remove restore result fixture");
}

#[test]
fn daemon_restart_restores_only_queued_migration_decisions() {
    let root = temporary_directory("migration-decision-restart");
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let accepted_json = serde_json::to_string(&IpcEventKind::Accepted).expect("accepted event");
    for (operation_id, created_at) in [("queued-decision", 100), ("running-decision", 101)] {
        let queued = super::QueuedMigrationDecision::new(
            operation_id.to_owned(),
            "restore-42".to_owned(),
            "bill".to_owned(),
            IpcMigrationDecision::Rollback,
        )
        .expect("migration decision");
        let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
            operation_id: operation_id.to_owned(),
            kind: "migration_decision".to_owned(),
            payload_json: queued.payload_json().expect("durable decision payload"),
            status: DaemonOperationStatus::Queued,
            created_at_unix_seconds: created_at,
            updated_at_unix_seconds: created_at,
        });
        store
            .enqueue_daemon_operation(&operation, &accepted_json, 256)
            .expect("persist queued decision");
    }
    drop(
        store
            .transition_daemon_operation(DaemonOperationTransitionOptions {
                operation_id: "running-decision",
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Running,
                updated_at_unix_seconds: 102,
                event_kind_json: None,
                event_retention_limit: 256,
            })
            .expect("claim running decision"),
    );

    let (commands, backups, prunes, restores, mut decisions) =
        restore_daemon_operation_queues(&mut store, 200).expect("restore daemon operations");

    assert_eq!(commands.len(), 0);
    assert_eq!(backups.len(), 0);
    assert_eq!(prunes.len(), 0);
    assert_eq!(restores.len(), 0);
    assert_eq!(decisions.len(), 1);
    assert_eq!(
        decisions
            .pop_front()
            .expect("restored migration decision")
            .operation_id(),
        "queued-decision"
    );
    let events = store.daemon_events().expect("daemon events");
    let interrupted = events.last().expect("interrupted terminal event");
    assert_eq!(interrupted.operation_id(), "running-decision");
    assert!(
        interrupted
            .kind_json()
            .contains("migration_decision_interrupted")
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove decision restart fixture");
}

#[test]
fn project_backup_intent_is_secret_free_and_survives_a_durable_round_trip() {
    let queued = QueuedProjectBackup::new(
        "backup-42".to_owned(),
        "bill".to_owned(),
        "database".to_owned(),
        "stackctl_bill_database".to_owned(),
        "postgres_database_and_role".to_owned(),
        "sha256:postgres-17".to_owned(),
    )
    .expect("valid backup intent");

    let payload = queued.payload_json().expect("durable backup payload");
    assert!(!payload.contains("secret-value"));
    let restored = QueuedProjectBackup::from_payload_json("backup-42".to_owned(), &payload)
        .expect("restore backup intent");

    assert_eq!(restored, queued);
}

#[test]
fn running_postgres_prune_replays_or_completes_from_atomic_state() {
    let root = temporary_directory("postgres-prune-restart");
    let accepted_json = serde_json::to_string(&IpcEventKind::Accepted).expect("accepted event");

    let replay_path = root.join("replay.sqlite3");
    let (queued, logical, credential) = postgres_prune_restart_fixture("prune-replay");
    let mut replay_store = SqliteStateStore::open(&replay_path).expect("replay store");
    replay_store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical state");
    replay_store
        .insert_credential_if_absent(&credential)
        .expect("credential state");
    replay_store
        .enqueue_daemon_operation(
            &DaemonOperationRecord::new(DaemonOperationRecordOptions {
                operation_id: queued.operation_id().to_owned(),
                kind: "postgres_prune".to_owned(),
                payload_json: queued.payload_json().expect("prune payload"),
                status: DaemonOperationStatus::Queued,
                created_at_unix_seconds: 100,
                updated_at_unix_seconds: 100,
            }),
            &accepted_json,
            256,
        )
        .expect("persist replayable prune");
    drop(
        replay_store
            .transition_daemon_operation(DaemonOperationTransitionOptions {
                operation_id: queued.operation_id(),
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Running,
                updated_at_unix_seconds: 101,
                event_kind_json: None,
                event_retention_limit: 256,
            })
            .expect("claim replayable prune"),
    );

    let (_, _, mut prunes, _, _) =
        restore_daemon_operation_queues(&mut replay_store, 200).expect("restore replayable prune");

    assert_eq!(prunes.len(), 1);
    assert_eq!(
        prunes.pop_front().expect("replayed prune").operation_id(),
        "prune-replay"
    );
    assert_eq!(
        replay_store
            .active_daemon_operations()
            .expect("requeued operation")[0]
            .status(),
        DaemonOperationStatus::Queued
    );

    let complete_path = root.join("complete.sqlite3");
    let (completed, _, _) = postgres_prune_restart_fixture("prune-complete");
    let mut complete_store = SqliteStateStore::open(&complete_path).expect("complete store");
    complete_store
        .enqueue_daemon_operation(
            &DaemonOperationRecord::new(DaemonOperationRecordOptions {
                operation_id: completed.operation_id().to_owned(),
                kind: "postgres_prune".to_owned(),
                payload_json: completed.payload_json().expect("completed payload"),
                status: DaemonOperationStatus::Queued,
                created_at_unix_seconds: 100,
                updated_at_unix_seconds: 100,
            }),
            &accepted_json,
            256,
        )
        .expect("persist completed prune");
    drop(
        complete_store
            .transition_daemon_operation(DaemonOperationTransitionOptions {
                operation_id: completed.operation_id(),
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Running,
                updated_at_unix_seconds: 101,
                event_kind_json: None,
                event_retention_limit: 256,
            })
            .expect("claim completed prune"),
    );

    let (_, _, prunes, _, _) =
        restore_daemon_operation_queues(&mut complete_store, 200).expect("restore completed prune");

    assert_eq!(prunes.len(), 0);
    assert!(
        complete_store
            .active_daemon_operations()
            .expect("no active completed prune")
            .is_empty()
    );
    assert!(
        complete_store
            .daemon_events()
            .expect("completion event")
            .last()
            .expect("terminal event")
            .kind_json()
            .contains("completed")
    );

    drop(replay_store);
    drop(complete_store);
    std::fs::remove_dir_all(root).expect("remove restart fixture");
}

#[test]
fn queued_postgres_backup_resolves_exact_owned_state_and_verifies_an_artifact() {
    let backup_root = temporary_directory("queued-postgres-backup");
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "postgres-container",
        "install-1",
        "postgres-17",
        "sha256:postgres-17",
    )]);
    let operation = QueuedProjectBackup::new(
        "backup-42".to_owned(),
        "bill".to_owned(),
        "database".to_owned(),
        "stackctl_bill_database".to_owned(),
        "postgres_database_and_role".to_owned(),
        "sha256:postgres-17".to_owned(),
    )
    .expect("valid backup intent");
    let logical_resource = crate::control_plane::state::LogicalResourceRecord::new(
        crate::control_plane::state::LogicalResourceRecordOptions {
            logical_resource_id: "stackctl_bill_database".to_owned(),
            shared_resource_id: "postgres-17".to_owned(),
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            kind: "postgres_database_and_role".to_owned(),
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            desired_revision: "sha256:desired".to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        },
    );
    let credential = crate::control_plane::state::CredentialRecord::new(
        crate::control_plane::state::CredentialRecordOptions {
            credential_id: "bill/database/primary".to_owned(),
            project_id: Some("bill".to_owned()),
            service_id: "database".to_owned(),
            username: "credential-user".to_owned(),
            secret: "secret-value".to_owned(),
            lifecycle: crate::control_plane::state::CredentialLifecycle::Active,
        },
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Ok(logical_resource),
            credential: Ok(credential),
            administrator: Ok(None),
            physical_resource: Ok(None),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 40_000,
            timeout: Duration::from_secs(30),
        },
    ));

    let backup = result.outcome().as_ref().expect("verified backup");
    assert_eq!(backup.artifact_size_bytes(), 10);
    assert_eq!(engine.containers(), ["postgres-container"]);
    assert_eq!(
        engine.command_environments()[0].get("PGPASSWORD"),
        Some(&"secret-value".to_owned())
    );
    let recovery_point = Path::new(backup.reference());
    assert!(recovery_point.join("artifact.bin").is_file());
    assert!(recovery_point.join("manifest.json").is_file());

    std::fs::remove_dir_all(backup_root).expect("remove backup fixture");
}

#[test]
fn queued_project_volume_backup_resolves_owned_service_and_quiesces_it() {
    let backup_root = temporary_directory("queued-project-volume-backup");
    let service_metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::ProjectService,
            project_id: Some("bill".to_owned()),
            compatibility_fingerprint: "sha256:search-3".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("service metadata")
    .with_resource_id("search")
    .expect("service identity");
    let volume_metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::Volume,
            project_id: Some("bill".to_owned()),
            compatibility_fingerprint: "sha256:search-3".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("volume metadata")
    .with_resource_id("search")
    .expect("volume identity");
    let engine = RecordingProjectCommandEngine::new(vec![
        crate::control_plane::engine::ObservedContainer::new(
            crate::control_plane::engine::ContainerId::new("search-container"),
            service_metadata.labels(),
        ),
    ])
    .with_observed_volumes(vec![crate::control_plane::engine::ObservedVolume::new(
        "stackctl-bill-search-data",
        volume_metadata.labels(),
    )])
    .with_volume_archive(b"project volume tar".to_vec());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "stackctl-bill-search-data".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:search-3".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("search");
    let operation = QueuedProjectBackup::new(
        "backup-volume".to_owned(),
        "bill".to_owned(),
        "search".to_owned(),
        resource.resource_id().to_owned(),
        resource.kind().to_owned(),
        resource.compatibility_fingerprint().to_owned(),
    )
    .expect("volume backup intent");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Err(crate::control_plane::engine::EngineError::InvalidRequest {
                detail: "volume has no logical resource".to_owned(),
            }),
            credential: Err(crate::control_plane::engine::EngineError::InvalidRequest {
                detail: "volume has no credential".to_owned(),
            }),
            administrator: Ok(None),
            physical_resource: Ok(Some(resource)),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 40_100,
            timeout: Duration::from_secs(30),
        },
    ));

    let backup = result.outcome().as_ref().expect("verified volume backup");
    assert_eq!(backup.artifact_size_bytes(), 18);
    assert_eq!(engine.stopped(), ["search-container"]);
    assert_eq!(engine.lifecycle_started(), ["search-container"]);
    std::fs::remove_dir_all(backup_root).expect("remove volume backup fixture");
}

#[test]
fn queued_project_volume_restore_records_safety_and_recreates_exact_target() {
    use sha2::Digest as _;

    let root = temporary_directory("queued-project-volume-restore");
    let backup_root = root.join("backups");
    let source = ProjectSource::new(
        root.join("bill"),
        root.join("bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  aws:\n    preset: localstack\n    version: '4'\n    image: localstack/localstack@sha256:{}\n",
            "c".repeat(64)
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
    .expect("dedicated restore plan");
    let target = plan
        .dedicated_services()
        .first()
        .expect("dedicated service")
        .clone();
    let volume = target.volume().expect("retained volume");
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: volume.name().to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: volume.metadata().compatibility_fingerprint().to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: volume.metadata().desired_revision().to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("aws");
    let identity =
        crate::control_plane::retention::BackupResourceIdentity::from_resource(&resource);
    let selected = crate::control_plane::retention::store_backup_artifact_for_identity(
        &identity,
        b"selected volume archive",
        80_000,
        &backup_root,
    )
    .expect("selected volume backup");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "volume-backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "aws".to_owned(),
        logical_resource_id: resource.resource_id().to_owned(),
        resource_kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        reference: selected.recovery_point().display().to_string(),
        artifact_sha256: hex::encode(sha2::Sha256::digest(b"selected volume archive")),
        artifact_size_bytes: 23,
        created_at_unix_seconds: 80_000,
        verified_at_unix_seconds: 80_000,
    })
    .expect("selected recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("restore state");
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("persist volume resource");
    store
        .record_recovery_point(&recovery)
        .expect("catalog selected recovery");
    drop(store);
    let engine = RecordingProjectCommandEngine::new(vec![
        crate::control_plane::engine::ObservedContainer::new(
            crate::control_plane::engine::ContainerId::new("localstack-container"),
            target.request().metadata().labels(),
        ),
    ])
    .with_observed_volumes(vec![crate::control_plane::engine::ObservedVolume::new(
        volume.name(),
        volume.metadata().labels(),
    )])
    .with_volume_archive(b"current volume archive".to_vec());
    let operation = QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
        operation_id: "restore-volume-42".to_owned(),
        recovery_point_id: recovery.recovery_point_id().to_owned(),
        project_id: "bill".to_owned(),
        service_id: "aws".to_owned(),
        logical_resource_id: resource.resource_id().to_owned(),
        kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
    })
    .expect("volume restore intent");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_restore(
        engine.clone(),
        FixedRestoreEntropy(0xaa),
        ProjectRestoreExecutionOptions {
            operation,
            target: ProjectRestoreTargetPlan::Dedicated(target.clone()),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 80_001,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(result.outcome(), &Ok(MigrationExecutionResult::Confirmed));
    assert_eq!(
        *engine
            .execution
            .volume_upload
            .lock()
            .expect("uploaded restore archive"),
        b"selected volume archive"
    );
    assert_eq!(engine.created(), [target.request().name()]);
    assert_eq!(
        engine
            .execution
            .created_volumes
            .lock()
            .expect("created restore volumes")
            .as_slice(),
        [volume.name()]
    );
    let recovery_points = SqliteStateStore::open(&database_path)
        .expect("reopen restore state")
        .recovery_points("bill")
        .expect("load recovery points");
    assert!(
        recovery_points
            .iter()
            .any(|point| point.recovery_point_id() == "restore-volume-42-pre-restore")
    );
    std::fs::remove_dir_all(root).expect("remove volume restore fixture");
}

#[test]
fn queued_rabbitmq_backup_exports_and_archives_exact_empty_vhost() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let backup_root = temporary_directory("queued-rabbitmq-backup");
    let fingerprint = format!("sha256:{}", "e".repeat(64));
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "rabbitmq-container",
        "install-1",
        "rabbitmq-4",
        &fingerprint,
    )])
    .with_observed_volumes(vec![observed_shared_volume(
        "rabbitmq-4-data",
        "install-1",
        "rabbitmq-4",
        &fingerprint,
    )]);
    let operation = QueuedProjectBackup::new(
        "backup-rabbitmq".to_owned(),
        "bill".to_owned(),
        "database".to_owned(),
        "bill/database/rabbitmq".to_owned(),
        "rabbitmq_vhost_user".to_owned(),
        fingerprint.clone(),
    )
    .expect("valid RabbitMQ backup intent");
    let logical_resource = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database/rabbitmq".to_owned(),
        shared_resource_id: "rabbitmq-4-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "rabbitmq_vhost_user".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/rabbitmq".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "rabbit-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Ok(logical_resource),
            credential: Ok(credential),
            administrator: Ok(None),
            physical_resource: Ok(None),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 40_000,
            timeout: Duration::from_secs(30),
        },
    ));

    let backup = result.outcome().as_ref().expect("verified RabbitMQ backup");
    let calls = engine.command_arguments();
    assert_eq!(calls[0], ["rabbitmqctl", "suspend_listeners"]);
    assert_eq!(
        calls[1][..3],
        ["rabbitmqctl", "close_all_connections", "--global"]
    );
    assert_eq!(
        calls[2],
        [
            "rabbitmqctl",
            "list_queues",
            "--vhost",
            "stackctl_bill_database",
            "name",
            "messages",
            "messages_persistent",
            "durable",
            "type",
            "--no-table-headers",
        ]
    );
    assert_eq!(calls[3][0], "sh");
    assert!(calls[3][2].contains("export_definitions"));
    assert!(calls[3][2].contains("STACKCTL_VHOST"));
    assert!(calls[4][2].contains("msg_stores/vhosts"));
    assert!(!format!("{calls:?}").contains("rabbit-secret"));
    assert!(
        engine.command_environments()[3]["STACKCTL_DEFINITIONS_FILE"]
            .contains("stackctl_bill_database-40000")
    );
    assert!(backup.artifact_size_bytes() > 10);
    assert_eq!(engine.stopped(), ["rabbitmq-container"]);
    assert_eq!(engine.lifecycle_started(), ["rabbitmq-container"]);
    assert_eq!(
        engine.volume_subpath_downloads(),
        ["mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P"]
    );
    assert!(Path::new(backup.reference()).join("artifact.bin").is_file());
    assert!(
        Path::new(backup.reference())
            .join("manifest.json")
            .is_file()
    );

    std::fs::remove_dir_all(backup_root).expect("remove RabbitMQ backup fixture");
}

#[test]
fn queued_rabbitmq_backup_quiesces_broker_and_archives_non_empty_vhost() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let backup_root = temporary_directory("queued-rabbitmq-non-empty-backup");
    let fingerprint = format!("sha256:{}", "f".repeat(64));
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "rabbitmq-container",
        "install-1",
        "rabbitmq-4",
        &fingerprint,
    )])
    .with_observed_volumes(vec![observed_shared_volume(
        "rabbitmq-4-data",
        "install-1",
        "rabbitmq-4",
        &fingerprint,
    )])
    .with_rabbitmq_queue_output(b"jobs\t2\t2\ttrue\tclassic\n".to_vec())
    .with_volume_archive(b"message-store-tar".to_vec());
    let operation = QueuedProjectBackup::new(
        "backup-rabbitmq-non-empty".to_owned(),
        "bill".to_owned(),
        "database".to_owned(),
        "bill/database/rabbitmq".to_owned(),
        "rabbitmq_vhost_user".to_owned(),
        fingerprint.clone(),
    )
    .expect("valid RabbitMQ backup intent");
    let logical_resource = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database/rabbitmq".to_owned(),
        shared_resource_id: "rabbitmq-4-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "rabbitmq_vhost_user".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/rabbitmq".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "rabbit-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Ok(logical_resource),
            credential: Ok(credential),
            administrator: Ok(None),
            physical_resource: Ok(None),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 40_000,
            timeout: Duration::from_secs(30),
        },
    ));

    let backup = result
        .outcome()
        .as_ref()
        .expect("non-empty RabbitMQ vhost backup");
    assert_eq!(engine.stopped(), ["rabbitmq-container"]);
    assert_eq!(engine.lifecycle_started(), ["rabbitmq-container"]);
    assert_eq!(
        engine.volume_subpath_downloads(),
        ["mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P"]
    );
    assert!(backup.artifact_size_bytes() > b"message-store-tar".len() as u64);
    assert!(Path::new(backup.reference()).join("artifact.bin").is_file());

    std::fs::remove_dir_all(backup_root).expect("remove RabbitMQ backup fixture");
}

#[test]
fn queued_minio_backup_streams_an_unversioned_project_bucket() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let backup_root = temporary_directory("queued-minio-backup");
    let fingerprint = format!("sha256:{}", "a".repeat(64));
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "minio-container",
        "install-1",
        "minio-1",
        &fingerprint,
    )]);
    let operation = QueuedProjectBackup::new(
        "backup-minio".to_owned(),
        "bill".to_owned(),
        "files".to_owned(),
        "bill/files/object-store".to_owned(),
        "minio_bucket_policy".to_owned(),
        fingerprint.clone(),
    )
    .expect("valid MinIO backup intent");
    let logical_resource = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/files/object-store".to_owned(),
        shared_resource_id: "minio-1".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "files".to_owned(),
        kind: "minio_bucket_policy".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/files/object-store".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "files".to_owned(),
        username: "st_bill_files".to_owned(),
        secret: "minio-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Ok(logical_resource),
            credential: Ok(credential),
            administrator: Ok(None),
            physical_resource: Ok(None),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 40_000,
            timeout: Duration::from_secs(30),
        },
    ));

    let backup = result.outcome().as_ref().expect("verified MinIO backup");
    let calls = engine.command_arguments();
    assert_eq!(calls.len(), 2);
    assert!(calls[0][2].contains("version info"));
    assert!(calls[1][2].contains(" mirror "));
    assert!(calls[1][2].contains("tar -C"));
    assert!(!format!("{calls:?}").contains("minio-secret"));
    assert!(
        engine.command_environments()[1]["STACKCTL_EXPORT_DIR"]
            .contains("stackctl-bill-files-40000")
    );
    assert_eq!(backup.artifact_size_bytes(), 10);

    std::fs::remove_dir_all(backup_root).expect("remove MinIO backup fixture");
}

#[test]
fn queued_redis_backup_uses_the_shared_administrator_and_exact_prefix() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let backup_root = temporary_directory("queued-redis-backup");
    let fingerprint = format!("sha256:{}", "a".repeat(64));
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "redis-container",
        "install-1",
        "redis-8",
        &fingerprint,
    )]);
    let operation = QueuedProjectBackup::new(
        "backup-redis".to_owned(),
        "bill".to_owned(),
        "cache".to_owned(),
        "bill/cache/redis".to_owned(),
        "redis_acl_prefix".to_owned(),
        fingerprint.clone(),
    )
    .expect("valid Redis backup intent");
    let logical_resource = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/cache/redis".to_owned(),
        shared_resource_id: "redis-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        kind: "redis_acl_prefix".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/cache/redis".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "cache".to_owned(),
        username: "st_bill_cache".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/redis-bootstrap", "a".repeat(64)),
        project_id: None,
        service_id: "redis".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "admin-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Ok(logical_resource),
            credential: Ok(credential),
            administrator: Ok(Some(administrator)),
            physical_resource: Ok(None),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 40_000,
            timeout: Duration::from_secs(30),
        },
    ));

    let backup = result.outcome().as_ref().expect("verified Redis backup");
    let arguments = &engine.command_arguments()[0];
    assert_eq!(arguments[0], "redis-cli");
    assert!(arguments.contains(&"stackctl:bill:cache:".to_owned()));
    assert_eq!(
        engine.command_environments()[0]["REDISCLI_AUTH"],
        "admin-secret"
    );
    assert!(!format!("{arguments:?}").contains("admin-secret"));
    assert!(Path::new(backup.reference()).join("artifact.bin").is_file());

    std::fs::remove_dir_all(backup_root).expect("remove Redis backup fixture");
}

#[test]
fn queued_mysql_backup_streams_exact_verified_logical_recovery_point() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let backup_root = temporary_directory("queued-mysql-backup");
    let fingerprint = format!("sha256:{}", "b".repeat(64));
    let observed = observed_shared_service("mysql-container", "install-1", "mysql-8", &fingerprint);
    let container =
        crate::control_plane::engine::reconstruct_owned_container(&observed, "install-1", 8)
            .expect("owned MySQL container");
    let engine = RecordingProjectCommandEngine::new(vec![observed]);
    let operation = QueuedProjectBackup::new(
        "backup-mysql".to_owned(),
        "bill".to_owned(),
        "database".to_owned(),
        "stackctl_bill_database".to_owned(),
        "mysql_database".to_owned(),
        fingerprint.clone(),
    )
    .expect("valid MySQL backup intent");
    let logical_resource = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "mysql-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mysql_database".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mysql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "secret-value".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Ok(logical_resource.clone()),
            credential: Ok(credential.clone()),
            administrator: Ok(None),
            physical_resource: Ok(None),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 40_000,
            timeout: Duration::from_secs(30),
        },
    ));

    let backup = result.outcome().as_ref().expect("verified MySQL backup");
    assert_eq!(backup.artifact_size_bytes(), 10);
    assert_eq!(engine.containers(), ["mysql-container"]);
    assert_eq!(engine.command_arguments()[0][0], "mysqldump");
    assert_eq!(
        engine.command_environments()[0].get("MYSQL_PWD"),
        Some(&"secret-value".to_owned())
    );
    assert!(!format!("{:?}", engine.command_arguments()).contains("secret-value"));
    let recovery_point = Path::new(backup.reference());
    assert!(recovery_point.join("artifact.bin").is_file());
    assert!(recovery_point.join("manifest.json").is_file());

    let checkpoint = MigrationRecord::new(MigrationRecordOptions {
        migration_id: "restore-mysql".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:desired".to_owned(),
        target_revision: "sha256:target".to_owned(),
        source_compatibility_fingerprint: logical_resource.compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: logical_resource.compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::TargetProvisioned,
        backup_reference: Some(backup.reference().to_owned()),
        backup_artifact_sha256: Some(backup.artifact_sha256().to_owned()),
        backup_artifact_size_bytes: Some(backup.artifact_size_bytes()),
        target_resource_id: Some("stackctl_bill_database".to_owned()),
        rollback_reference: Some("mysql-8".to_owned()),
        updated_at_unix_seconds: 40_001,
    })
    .expect("MySQL restore checkpoint");

    runtime
        .block_on(restore_mysql_database(
            &engine,
            &container,
            &MySqlRestoreOptions {
                flavor: crate::control_plane::shared_infrastructure::MySqlFlavor::MySql,
                checkpoint: &checkpoint,
                source_logical_resource: &logical_resource,
                credential: &credential,
                installation_id: "install-1",
                target_database_name: "stackctl_bill_database",
                verified_at_unix_seconds: 40_002,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("verified MySQL restore");

    assert_eq!(engine.command_arguments()[1][0], "mysql");
    assert!(
        engine.command_arguments()[1].contains(&"--database=stackctl_bill_database".to_owned())
    );
    assert_eq!(
        engine.command_environments()[1].get("MYSQL_PWD"),
        Some(&"secret-value".to_owned())
    );
    assert_eq!(engine.command_inputs()[1], b"installed\n");

    std::fs::remove_dir_all(backup_root).expect("remove backup fixture");
}

#[test]
fn queued_mongodb_backup_streams_exact_verified_logical_recovery_point() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let backup_root = temporary_directory("queued-mongodb-backup");
    let fingerprint = format!("sha256:{}", "c".repeat(64));
    let observed =
        observed_shared_service("mongodb-container", "install-1", "mongodb-8", &fingerprint);
    let target = observed_migration_target(
        "mongodb-target",
        "install-1",
        "bill",
        "restore-mongodb",
        &fingerprint,
    );
    let container =
        crate::control_plane::engine::reconstruct_owned_container(&target, "install-1", 8)
            .expect("owned MongoDB target");
    let engine = RecordingProjectCommandEngine::new(vec![observed]);
    let operation = QueuedProjectBackup::new(
        "backup-mongodb".to_owned(),
        "bill".to_owned(),
        "database".to_owned(),
        "stackctl_bill_database".to_owned(),
        "mongodb_database".to_owned(),
        fingerprint.clone(),
    )
    .expect("valid MongoDB backup intent");
    let logical_resource = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "mongodb-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mongodb_database".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mongodb".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "mongo secret:/?#[]@!$&'()*+,;=".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Ok(logical_resource.clone()),
            credential: Ok(credential.clone()),
            administrator: Ok(None),
            physical_resource: Ok(None),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 41_000,
            timeout: Duration::from_secs(30),
        },
    ));

    let backup = result.outcome().as_ref().expect("verified MongoDB backup");
    assert_eq!(backup.artifact_size_bytes(), 10);
    assert_eq!(engine.command_arguments()[0][0], "sh");
    assert!(engine.command_arguments()[0][2].contains("mongodump"));
    assert!(!format!("{:?}", engine.command_arguments()).contains("mongo secret"));
    let command_environments = engine.command_environments();
    let uri = command_environments[0]
        .get("STACKCTL_MONGODB_URI")
        .expect("runtime-only MongoDB URI");
    assert!(uri.starts_with("mongodb://st_bill_database:"));
    assert!(!uri.contains("mongo secret"));
    let recovery_point = Path::new(backup.reference());
    assert!(recovery_point.join("artifact.bin").is_file());
    assert!(recovery_point.join("manifest.json").is_file());

    let checkpoint = MigrationRecord::new(MigrationRecordOptions {
        migration_id: "restore-mongodb".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:desired".to_owned(),
        target_revision: "sha256:target".to_owned(),
        source_compatibility_fingerprint: logical_resource.compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: logical_resource.compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::TargetProvisioned,
        backup_reference: Some(backup.reference().to_owned()),
        backup_artifact_sha256: Some(backup.artifact_sha256().to_owned()),
        backup_artifact_size_bytes: Some(backup.artifact_size_bytes()),
        target_resource_id: Some("stackctl_bill_database".to_owned()),
        rollback_reference: Some("mongodb-8".to_owned()),
        updated_at_unix_seconds: 41_001,
    })
    .expect("MongoDB restore checkpoint");
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "migration/restore-mongodb/mongodb-bootstrap".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "mongodb".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "root secret:/?#[]@".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });

    runtime
        .block_on(restore_mongodb_database(
            &engine,
            &container,
            &MongoDbRestoreOptions {
                checkpoint: &checkpoint,
                source_logical_resource: &logical_resource,
                administrator: &administrator,
                installation_id: "install-1",
                target_database_name: "stackctl_bill_database",
                verified_at_unix_seconds: 41_002,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("verified MongoDB restore");

    assert_eq!(engine.command_arguments()[1][0], "sh");
    assert!(engine.command_arguments()[1][2].contains("mongorestore"));
    assert!(!format!("{:?}", engine.command_arguments()).contains("root secret"));
    assert_eq!(engine.command_inputs()[1], b"installed\n");
    let restore_environments = engine.command_environments();
    let restore_uri = restore_environments[1]
        .get("STACKCTL_MONGODB_URI")
        .expect("runtime-only administrator URI");
    assert!(restore_uri.contains("authSource=admin"));
    assert!(!restore_uri.contains("root secret"));

    let restored_checkpoint = MigrationRecord::new(MigrationRecordOptions {
        migration_id: checkpoint.migration_id().to_owned(),
        project_id: checkpoint.project_id().to_owned(),
        source_revision: checkpoint.source_revision().to_owned(),
        target_revision: checkpoint.target_revision().to_owned(),
        source_compatibility_fingerprint: checkpoint.source_compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: checkpoint.target_compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::DataRestored,
        backup_reference: checkpoint.backup_reference().map(str::to_owned),
        backup_artifact_sha256: checkpoint.backup_artifact_sha256().map(str::to_owned),
        backup_artifact_size_bytes: checkpoint.backup_artifact_size_bytes(),
        target_resource_id: checkpoint.target_resource_id().map(str::to_owned),
        rollback_reference: checkpoint.rollback_reference().map(str::to_owned),
        updated_at_unix_seconds: 41_003,
    })
    .expect("restored MongoDB checkpoint");

    runtime
        .block_on(verify_mongodb_target(
            &engine,
            &container,
            &MongoDbVerifyTargetOptions {
                checkpoint: &restored_checkpoint,
                credential: &credential,
                installation_id: "install-1",
                target_database_name: "stackctl_bill_database",
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("tenant-authenticated MongoDB target verification");

    assert!(engine.command_arguments()[2][2].contains("mongosh"));
    let verification_environments = engine.command_environments();
    let verification_uri = verification_environments[2]
        .get("STACKCTL_MONGODB_URI")
        .expect("runtime-only tenant URI");
    assert!(verification_uri.contains("authSource=stackctl_bill_database"));
    assert!(!verification_uri.contains("mongo secret"));

    std::fs::remove_dir_all(backup_root).expect("remove MongoDB backup fixture");
}

#[test]
fn queued_sql_server_backup_streams_exact_verified_native_recovery_point() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let backup_root = temporary_directory("queued-sqlserver-backup");
    let fingerprint = format!("sha256:{}", "d".repeat(64));
    let observed = observed_shared_service(
        "sqlserver-container",
        "install-1",
        "sqlserver-2022",
        &fingerprint,
    );
    let engine = RecordingProjectCommandEngine::new(vec![observed]);
    let operation = QueuedProjectBackup::new(
        "backup-sqlserver".to_owned(),
        "bill".to_owned(),
        "database".to_owned(),
        "stackctl_bill_database".to_owned(),
        "sqlserver_database".to_owned(),
        fingerprint.clone(),
    )
    .expect("valid SQL Server backup intent");
    let logical_resource = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "sqlserver-2022".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "sqlserver_database".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/sqlserver".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "ProjectSecret1".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_project_backup(
        engine.clone(),
        ProjectBackupExecutionOptions {
            operation,
            logical_resource: Ok(logical_resource.clone()),
            credential: Ok(credential.clone()),
            administrator: Ok(None),
            physical_resource: Ok(None),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            backup_root: backup_root.clone(),
            created_at_unix_seconds: 42_000,
            timeout: Duration::from_secs(120),
        },
    ));

    let backup = result
        .outcome()
        .as_ref()
        .expect("verified SQL Server backup");
    assert_eq!(backup.artifact_size_bytes(), 10);
    assert_eq!(engine.command_arguments()[0][0], "sh");
    assert!(engine.command_arguments()[0][2].contains("STACKCTL_BACKUP_FILE"));
    assert!(!format!("{:?}", engine.command_arguments()).contains("ProjectSecret1"));
    let environment = &engine.command_environments()[0];
    assert_eq!(
        environment.get("SQLCMDPASSWORD"),
        Some(&"ProjectSecret1".to_owned())
    );
    assert!(
        environment
            .get("STACKCTL_BACKUP_SQL")
            .expect("native backup SQL")
            .contains("BACKUP DATABASE [stackctl_bill_database]")
    );
    let recovery_point = Path::new(backup.reference());
    assert!(recovery_point.join("artifact.bin").is_file());
    assert!(recovery_point.join("manifest.json").is_file());

    let checkpoint = MigrationRecord::new(MigrationRecordOptions {
        migration_id: "restore-sqlserver".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:desired".to_owned(),
        target_revision: "sha256:target".to_owned(),
        source_compatibility_fingerprint: logical_resource.compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: logical_resource.compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::TargetProvisioned,
        backup_reference: Some(backup.reference().to_owned()),
        backup_artifact_sha256: Some(backup.artifact_sha256().to_owned()),
        backup_artifact_size_bytes: Some(backup.artifact_size_bytes()),
        target_resource_id: Some("stackctl_bill_database".to_owned()),
        rollback_reference: Some("sqlserver-2022".to_owned()),
        updated_at_unix_seconds: 42_001,
    })
    .expect("SQL Server restore checkpoint");
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "migration/restore-sqlserver/sqlserver-bootstrap".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "sqlserver".to_owned(),
        username: "sa".to_owned(),
        secret: "AdministratorSecret1".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let target = observed_migration_target(
        "sqlserver-target",
        "install-1",
        "bill",
        "restore-sqlserver",
        logical_resource.compatibility_fingerprint(),
    );
    let target = crate::control_plane::engine::reconstruct_owned_container(&target, "install-1", 8)
        .expect("owned SQL Server target");

    runtime
        .block_on(restore_sql_server_database(
            &engine,
            &target,
            &SqlServerRestoreOptions {
                checkpoint: &checkpoint,
                source_logical_resource: &logical_resource,
                credential: &credential,
                administrator: &administrator,
                installation_id: "install-1",
                target_database_name: "stackctl_bill_database",
                verified_at_unix_seconds: 42_002,
                timeout: Duration::from_secs(120),
            },
        ))
        .expect("verified native SQL Server restore");

    assert_eq!(engine.command_arguments()[1][0], "sh");
    assert!(engine.command_arguments()[1][2].contains("STACKCTL_RESTORE_FILE"));
    assert_eq!(engine.command_inputs()[1], b"installed\n");
    let restored_checkpoint = MigrationRecord::new(MigrationRecordOptions {
        migration_id: checkpoint.migration_id().to_owned(),
        project_id: checkpoint.project_id().to_owned(),
        source_revision: checkpoint.source_revision().to_owned(),
        target_revision: checkpoint.target_revision().to_owned(),
        source_compatibility_fingerprint: checkpoint.source_compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: checkpoint.target_compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::DataRestored,
        backup_reference: checkpoint.backup_reference().map(str::to_owned),
        backup_artifact_sha256: checkpoint.backup_artifact_sha256().map(str::to_owned),
        backup_artifact_size_bytes: checkpoint.backup_artifact_size_bytes(),
        target_resource_id: checkpoint.target_resource_id().map(str::to_owned),
        rollback_reference: checkpoint.rollback_reference().map(str::to_owned),
        updated_at_unix_seconds: 42_003,
    })
    .expect("restored SQL Server checkpoint");
    runtime
        .block_on(verify_sql_server_target(
            &engine,
            &target,
            &SqlServerVerifyTargetOptions {
                checkpoint: &restored_checkpoint,
                credential: &credential,
                installation_id: "install-1",
                target_database_name: "stackctl_bill_database",
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("tenant-authenticated SQL Server target verification");

    assert_eq!(
        engine.command_arguments()[2][0],
        "/opt/mssql-tools18/bin/sqlcmd"
    );
    assert_eq!(
        engine.command_environments()[2].get("SQLCMDPASSWORD"),
        Some(&"ProjectSecret1".to_owned())
    );

    std::fs::remove_dir_all(backup_root).expect("remove SQL Server backup fixture");
}

#[test]
fn queued_postgres_prune_revalidates_deletes_and_then_retires_state() {
    use crate::control_plane::retention::{
        BackupResourceIdentity, PostgresLogicalPrunePlan, PostgresLogicalPrunePlanOptions,
        store_backup_artifact_for_identity,
    };
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
        InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-postgres-prune");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "postgres-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/primary".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/postgres-17/postgresql-bootstrap".to_owned(),
        project_id: None,
        service_id: "postgresql".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment".to_owned(),
        values: BTreeMap::from([("DB_PASSWORD".to_owned(), "project-secret".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let backup = store_backup_artifact_for_identity(
        &BackupResourceIdentity::from_logical(&logical, "install-1"),
        b"postgres backup",
        39_000,
        &root.join("backups"),
    )
    .expect("stored recovery artifact");
    let recovery_point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: backup.recovery_point().display().to_string(),
        artifact_sha256: "3fe135eb41e568127d31f382e7ebb8350b001024ff1257d9b857d5c787cfc494"
            .to_owned(),
        artifact_size_bytes: 15,
        created_at_unix_seconds: 39_000,
        verified_at_unix_seconds: 39_100,
    })
    .expect("recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("installation");
    store.replace_project(&project).expect("project");
    store
        .record_logical_environment(std::slice::from_ref(&logical), &environment)
        .expect("logical environment");
    store
        .insert_credential_if_absent(&credential)
        .expect("project credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator credential");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    store
        .orphan_project(&project_path, 40_000)
        .expect("orphan project");
    let logical_resources = store.logical_resources().expect("logical resources");
    let credentials = store.credentials().expect("credentials");
    let recovery_points = store.recovery_points("bill").expect("recovery points");
    let plan = PostgresLogicalPrunePlan::new(PostgresLogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-42",
        project_registered: false,
        logical_resources: &logical_resources,
        credentials: &credentials,
        recovery_points: &recovery_points,
    })
    .expect("prune plan");
    let operation = QueuedPostgresPrune::new(
        "prune-42".to_owned(),
        &plan,
        plan.confirmation_token().to_owned(),
    )
    .expect("queued prune");
    drop(store);
    let mut drift_store = SqliteStateStore::open(&database_path).expect("drift state");
    let orphaned = drift_store.logical_resources().expect("orphaned state")[0].clone();
    let drifted = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: orphaned.logical_resource_id().to_owned(),
        shared_resource_id: orphaned.shared_resource_id().to_owned(),
        project_id: orphaned.project_id().to_owned(),
        service_id: orphaned.service_id().to_owned(),
        kind: orphaned.kind().to_owned(),
        compatibility_fingerprint: orphaned.compatibility_fingerprint().to_owned(),
        desired_revision: "sha256:drifted".to_owned(),
        lifecycle: orphaned.lifecycle(),
        orphaned_at_unix_seconds: orphaned.orphaned_at_unix_seconds(),
    });
    drift_store
        .upsert_logical_resources(std::slice::from_ref(&drifted))
        .expect("drift retained state");
    drop(drift_store);
    let stale_engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "postgres-container",
        "install-1",
        "postgres-17",
        "sha256:postgres-17",
    )]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let stale = runtime.block_on(execute_queued_postgres_prune(
        stale_engine.clone(),
        PostgresPruneExecutionOptions {
            operation: operation.clone(),
            state_database_path: database_path.clone(),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            verified_at_unix_seconds: 40_001,
            timeout: Duration::from_secs(30),
        },
    ));
    assert!(
        stale
            .outcome()
            .as_ref()
            .expect_err("drift must invalidate prune")
            .contains("stale")
    );
    assert_eq!(stale_engine.started(), 0);
    let mut repair_store = SqliteStateStore::open(&database_path).expect("repair state");
    repair_store
        .upsert_logical_resources(std::slice::from_ref(&orphaned))
        .expect("restore planned state");
    drop(repair_store);
    std::fs::write(backup.artifact_file(), b"tampered backup").expect("tamper recovery artifact");
    let tampered_engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "postgres-container",
        "install-1",
        "postgres-17",
        "sha256:postgres-17",
    )]);
    let tampered = runtime.block_on(execute_queued_postgres_prune(
        tampered_engine.clone(),
        PostgresPruneExecutionOptions {
            operation: operation.clone(),
            state_database_path: database_path.clone(),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            verified_at_unix_seconds: 40_001,
            timeout: Duration::from_secs(30),
        },
    ));
    assert!(
        tampered
            .outcome()
            .as_ref()
            .expect_err("tampered recovery must block prune")
            .contains("checksum")
    );
    assert_eq!(tampered_engine.started(), 0);
    std::fs::write(backup.artifact_file(), b"postgres backup").expect("repair test artifact");
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "postgres-container",
        "install-1",
        "postgres-17",
        "sha256:postgres-17",
    )]);
    let result = runtime.block_on(execute_queued_postgres_prune(
        engine.clone(),
        PostgresPruneExecutionOptions {
            operation,
            state_database_path: database_path.clone(),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            verified_at_unix_seconds: 40_002,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(result.outcome(), &Ok(()));
    assert_eq!(engine.containers(), ["postgres-container"]);
    assert_eq!(
        engine.command_environments()[0].get("PGPASSWORD"),
        Some(&"administrator-secret".to_owned())
    );
    let sql = String::from_utf8(engine.command_inputs()[0].clone()).expect("prune SQL");
    assert!(sql.contains("DROP DATABASE IF EXISTS stackctl_bill_database"));
    assert!(sql.contains("DROP ROLE IF EXISTS stackctl_bill_database"));
    assert!(!sql.contains("project-secret"));
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert!(
        store
            .logical_resources()
            .expect("logical retired")
            .is_empty()
    );
    assert_eq!(
        store.credentials().expect("only administrator remains"),
        vec![administrator]
    );
    assert!(
        store
            .managed_environments()
            .expect("environment retired")
            .is_empty()
    );
    assert_eq!(
        store.recovery_points("bill").expect("recovery retained"),
        vec![recovery_point]
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove prune fixture");
}

#[test]
fn queued_logical_prune_dispatches_mysql_adapter_and_retires_exact_state() {
    use crate::control_plane::retention::{LogicalPrunePlan, LogicalPrunePlanOptions};
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
        InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-mysql-prune");
    let fingerprint = format!("sha256:{}", "b".repeat(64));
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "mysql-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mysql_database".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mysql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/mysql-bootstrap", "b".repeat(64)),
        project_id: None,
        service_id: "mysql".to_owned(),
        username: "root".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let recovery_point = stored_logical_recovery(&root, &logical, "backup-mysql");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("installation");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("tenant credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator credential");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    let plan = LogicalPrunePlan::new(LogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-mysql",
        project_registered: false,
        logical_resources: std::slice::from_ref(&logical),
        credentials: std::slice::from_ref(&credential),
        recovery_points: std::slice::from_ref(&recovery_point),
    })
    .expect("MySQL prune plan");
    let operation = QueuedPostgresPrune::new(
        "prune-mysql".to_owned(),
        &plan,
        plan.confirmation_token().to_owned(),
    )
    .expect("queued MySQL prune");
    drop(store);
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "mysql-container",
        "install-1",
        "mysql-8",
        &fingerprint,
    )]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_postgres_prune(
        engine.clone(),
        PostgresPruneExecutionOptions {
            operation,
            state_database_path: database_path.clone(),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            verified_at_unix_seconds: 40_001,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(result.outcome(), &Ok(()));
    assert_eq!(engine.command_arguments()[0][0], "mysql");
    assert_eq!(
        engine.command_environments()[0].get("MYSQL_PWD"),
        Some(&"administrator-secret".to_owned())
    );
    let sql = String::from_utf8(engine.command_inputs()[0].clone()).expect("prune SQL");
    assert!(sql.contains("DROP DATABASE IF EXISTS `stackctl_bill_database`"));
    assert!(sql.contains("DROP USER IF EXISTS 'st_bill_database'@'%'"));
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert!(
        store
            .logical_resources()
            .expect("logical retired")
            .is_empty()
    );
    assert_eq!(
        store.credentials().expect("administrator retained"),
        vec![administrator]
    );
    assert_eq!(
        store.recovery_points("bill").expect("recovery retained"),
        vec![recovery_point]
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove prune fixture");
}

#[test]
fn queued_logical_prune_dispatches_mongodb_adapter_and_retires_exact_state() {
    use crate::control_plane::retention::{LogicalPrunePlan, LogicalPrunePlanOptions};
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
        InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-mongodb-prune");
    let fingerprint = format!("sha256:{}", "c".repeat(64));
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "mongodb-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mongodb_database".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mongodb".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/mongodb-bootstrap", "c".repeat(64)),
        project_id: None,
        service_id: "mongodb".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let recovery_point = stored_logical_recovery(&root, &logical, "backup-mongodb");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("installation");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("tenant credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator credential");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    let plan = LogicalPrunePlan::new(LogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-mongodb",
        project_registered: false,
        logical_resources: std::slice::from_ref(&logical),
        credentials: std::slice::from_ref(&credential),
        recovery_points: std::slice::from_ref(&recovery_point),
    })
    .expect("MongoDB prune plan");
    let operation = QueuedPostgresPrune::new(
        "prune-mongodb".to_owned(),
        &plan,
        plan.confirmation_token().to_owned(),
    )
    .expect("queued MongoDB prune");
    drop(store);
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "mongodb-container",
        "install-1",
        "mongodb-8",
        &fingerprint,
    )]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_postgres_prune(
        engine.clone(),
        PostgresPruneExecutionOptions {
            operation,
            state_database_path: database_path.clone(),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            verified_at_unix_seconds: 40_001,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(result.outcome(), &Ok(()));
    assert_eq!(
        engine.command_arguments()[0],
        ["mongosh", "--quiet", "--nodb"]
    );
    assert!(engine.command_environments()[0].is_empty());
    let script = String::from_utf8(engine.command_inputs()[0].clone()).expect("prune script");
    assert!(script.contains("dropUser(\"st_bill_database\")"));
    assert!(script.contains("dropDatabase()"));
    assert!(script.contains("administrator-secret"));
    assert!(!script.contains("project-secret"));
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert!(
        store
            .logical_resources()
            .expect("logical retired")
            .is_empty()
    );
    assert_eq!(
        store.credentials().expect("administrator retained"),
        vec![administrator]
    );
    assert_eq!(
        store.recovery_points("bill").expect("recovery retained"),
        vec![recovery_point]
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove prune fixture");
}

#[test]
fn queued_logical_prune_dispatches_sql_server_adapter_and_retires_exact_state() {
    use crate::control_plane::retention::{LogicalPrunePlan, LogicalPrunePlanOptions};
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
        InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-sqlserver-prune");
    let fingerprint = format!("sha256:{}", "d".repeat(64));
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "sqlserver-2022".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "sqlserver_database".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/sqlserver".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "ProjectSecret1".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/sqlserver-bootstrap", "d".repeat(64)),
        project_id: None,
        service_id: "sqlserver".to_owned(),
        username: "sa".to_owned(),
        secret: "AdministratorSecret1".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let recovery_point = stored_logical_recovery(&root, &logical, "backup-sqlserver");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("installation");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("tenant credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator credential");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    let plan = LogicalPrunePlan::new(LogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-sqlserver",
        project_registered: false,
        logical_resources: std::slice::from_ref(&logical),
        credentials: std::slice::from_ref(&credential),
        recovery_points: std::slice::from_ref(&recovery_point),
    })
    .expect("SQL Server prune plan");
    let operation = QueuedPostgresPrune::new(
        "prune-sqlserver".to_owned(),
        &plan,
        plan.confirmation_token().to_owned(),
    )
    .expect("queued SQL Server prune");
    drop(store);
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "sqlserver-container",
        "install-1",
        "sqlserver-2022",
        &fingerprint,
    )]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_postgres_prune(
        engine.clone(),
        PostgresPruneExecutionOptions {
            operation,
            state_database_path: database_path.clone(),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            verified_at_unix_seconds: 40_001,
            timeout: Duration::from_secs(60),
        },
    ));

    assert_eq!(result.outcome(), &Ok(()));
    assert_eq!(
        engine.command_arguments()[0][0],
        "/opt/mssql-tools18/bin/sqlcmd"
    );
    assert_eq!(
        engine.command_environments()[0].get("SQLCMDPASSWORD"),
        Some(&"AdministratorSecret1".to_owned())
    );
    let sql = String::from_utf8(engine.command_inputs()[0].clone()).expect("prune SQL");
    assert!(sql.contains("DROP DATABASE [stackctl_bill_database]"));
    assert!(sql.contains("DROP LOGIN [st_bill_database]"));
    assert!(!sql.contains("ProjectSecret1"));
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert!(
        store
            .logical_resources()
            .expect("logical retired")
            .is_empty()
    );
    assert_eq!(
        store.credentials().expect("administrator retained"),
        vec![administrator]
    );
    assert_eq!(
        store.recovery_points("bill").expect("recovery retained"),
        vec![recovery_point]
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove prune fixture");
}

#[test]
fn queued_logical_prune_dispatches_rabbitmq_adapter_and_retires_exact_state() {
    use crate::control_plane::retention::{LogicalPrunePlan, LogicalPrunePlanOptions};
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
        InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-rabbitmq-prune");
    let fingerprint = format!("sha256:{}", "e".repeat(64));
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database/rabbitmq".to_owned(),
        shared_resource_id: "rabbitmq-4".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "rabbitmq_vhost_user".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/rabbitmq".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let recovery_point = stored_logical_recovery(&root, &logical, "backup-rabbitmq");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("installation");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("tenant credential");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    let plan = LogicalPrunePlan::new(LogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-rabbitmq",
        project_registered: false,
        logical_resources: std::slice::from_ref(&logical),
        credentials: std::slice::from_ref(&credential),
        recovery_points: std::slice::from_ref(&recovery_point),
    })
    .expect("RabbitMQ prune plan");
    let operation = QueuedPostgresPrune::new(
        "prune-rabbitmq".to_owned(),
        &plan,
        plan.confirmation_token().to_owned(),
    )
    .expect("queued RabbitMQ prune");
    drop(store);
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "rabbitmq-container",
        "install-1",
        "rabbitmq-4",
        &fingerprint,
    )]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_postgres_prune(
        engine.clone(),
        PostgresPruneExecutionOptions {
            operation,
            state_database_path: database_path.clone(),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            verified_at_unix_seconds: 40_001,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(result.outcome(), &Ok(()));
    let calls = engine.command_arguments();
    assert_eq!(calls[0][..2], ["rabbitmqctl", "list_users"]);
    assert_eq!(calls[1], ["rabbitmqctl", "delete_user", "st_bill_database"]);
    assert_eq!(calls[2][..2], ["rabbitmqctl", "list_vhosts"]);
    assert_eq!(
        calls[3],
        ["rabbitmqctl", "delete_vhost", "stackctl_bill_database"]
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert!(
        store
            .logical_resources()
            .expect("logical retired")
            .is_empty()
    );
    assert!(store.credentials().expect("credential retired").is_empty());
    assert_eq!(
        store.recovery_points("bill").expect("recovery retained"),
        vec![recovery_point]
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove prune fixture");
}

#[test]
fn queued_logical_prune_dispatches_redis_adapter_and_retires_exact_state() {
    use crate::control_plane::retention::{LogicalPrunePlan, LogicalPrunePlanOptions};
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
        InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-redis-prune");
    let fingerprint = format!("sha256:{}", "a".repeat(64));
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/cache/redis".to_owned(),
        shared_resource_id: "redis-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        kind: "redis_acl_prefix".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/cache/redis".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "cache".to_owned(),
        username: "st_bill_cache".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/redis-bootstrap", "a".repeat(64)),
        project_id: None,
        service_id: "redis".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let recovery_point = stored_logical_recovery(&root, &logical, "backup-redis");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("installation");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("tenant credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator credential");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    let plan = LogicalPrunePlan::new(LogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "cache",
        recovery_point_id: "backup-redis",
        project_registered: false,
        logical_resources: std::slice::from_ref(&logical),
        credentials: std::slice::from_ref(&credential),
        recovery_points: std::slice::from_ref(&recovery_point),
    })
    .expect("Redis prune plan");
    let operation = QueuedPostgresPrune::new(
        "prune-redis".to_owned(),
        &plan,
        plan.confirmation_token().to_owned(),
    )
    .expect("queued Redis prune");
    drop(store);
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        "redis-container",
        "install-1",
        "redis-8",
        &fingerprint,
    )]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(execute_queued_postgres_prune(
        engine.clone(),
        PostgresPruneExecutionOptions {
            operation,
            state_database_path: database_path.clone(),
            installation_id: "install-1".to_owned(),
            schema_version: 8,
            verified_at_unix_seconds: 40_001,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(result.outcome(), &Ok(()));
    let calls = engine.command_arguments();
    assert_eq!(&calls[0][4..], ["ACL", "DELUSER", "st_bill_cache"]);
    assert_eq!(calls[1][4], "EVAL");
    assert!(calls[1][5].contains("UNLINK"));
    assert_eq!(calls[1].last(), Some(&"stackctl:bill:cache:".to_owned()));
    assert_eq!(
        engine.command_environments()[0]["REDISCLI_AUTH"],
        "administrator-secret"
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert!(
        store
            .logical_resources()
            .expect("logical retired")
            .is_empty()
    );
    assert_eq!(
        store.credentials().expect("administrator retained"),
        vec![administrator]
    );
    assert_eq!(
        store.recovery_points("bill").expect("recovery retained"),
        vec![recovery_point]
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove prune fixture");
}

#[test]
fn queued_postgres_restore_reconciles_target_and_reaches_reversible_cutover() {
    let root = temporary_directory("queued-postgres-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let image = concat!(
        "postgres@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[ProjectSource::new(
        project_path.clone(),
        project_path.join(".stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  database:\n    preset: postgres\n    version: \"17\"\n    image: {image}\n"
        ),
    )])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared instances")
        .pop()
        .expect("PostgreSQL instance");
    let fingerprint = shared.fingerprint().as_str().to_owned();
    let source_container_name = format!(
        "stackctl-shared-{}",
        fingerprint.strip_prefix("sha256:").expect("fingerprint")
    );
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        &source_container_name,
        "install-1",
        "postgres-source",
        &fingerprint,
    )]);
    let source = crate::control_plane::state::LogicalResourceRecord::new(
        crate::control_plane::state::LogicalResourceRecordOptions {
            logical_resource_id: "stackctl_bill_database".to_owned(),
            shared_resource_id: source_container_name.clone(),
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            kind: "postgres_database_and_role".to_owned(),
            compatibility_fingerprint: fingerprint.clone(),
            desired_revision: "sha256:source".to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        },
    );
    let source_credential = crate::control_plane::state::CredentialRecord::new(
        crate::control_plane::state::CredentialRecordOptions {
            credential_id: "bill/database/postgresql".to_owned(),
            project_id: Some("bill".to_owned()),
            service_id: "database".to_owned(),
            username: "stackctl_bill_database_role".to_owned(),
            secret: "project-secret".to_owned(),
            lifecycle: crate::control_plane::state::CredentialLifecycle::Active,
        },
    );
    let fingerprint_id = fingerprint.strip_prefix("sha256:").expect("fingerprint");
    let administrator = crate::control_plane::state::CredentialRecord::new(
        crate::control_plane::state::CredentialRecordOptions {
            credential_id: format!("shared/{fingerprint_id}/postgresql-bootstrap"),
            project_id: None,
            service_id: "postgresql".to_owned(),
            username: "stackctl_admin".to_owned(),
            secret: "source-admin".to_owned(),
            lifecycle: crate::control_plane::state::CredentialLifecycle::Active,
        },
    );
    let source_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:source".to_owned(),
        values: BTreeMap::from([
            ("DB_HOST".to_owned(), source_container_name.clone()),
            (
                "DB_DATABASE".to_owned(),
                "stackctl_bill_database".to_owned(),
            ),
            (
                "DB_USERNAME".to_owned(),
                source_credential.username().to_owned(),
            ),
            (
                "DB_PASSWORD".to_owned(),
                source_credential.secret().to_owned(),
            ),
        ]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let backup_root = root.join("backups");
    let backup = runtime
        .block_on(execute_queued_project_backup(
            engine.clone(),
            ProjectBackupExecutionOptions {
                operation: QueuedProjectBackup::new(
                    "backup-42".to_owned(),
                    "bill".to_owned(),
                    "database".to_owned(),
                    source.logical_resource_id().to_owned(),
                    source.kind().to_owned(),
                    fingerprint.clone(),
                )
                .expect("backup intent"),
                logical_resource: Ok(source.clone()),
                credential: Ok(source_credential.clone()),
                administrator: Ok(None),
                physical_resource: Ok(None),
                installation_id: "install-1".to_owned(),
                schema_version: 8,
                backup_root: backup_root.clone(),
                created_at_unix_seconds: 40_000,
                timeout: Duration::from_secs(30),
            },
        ))
        .outcome()
        .as_ref()
        .expect("verified backup")
        .clone();
    let recovery_point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: source.logical_resource_id().to_owned(),
        resource_kind: source.kind().to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        reference: backup.reference().to_owned(),
        artifact_sha256: backup.artifact_sha256().to_owned(),
        artifact_size_bytes: backup.artifact_size_bytes(),
        created_at_unix_seconds: 40_000,
        verified_at_unix_seconds: 40_001,
    })
    .expect("recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .replace_project(&ProjectRecord::new(
            project_path,
            "bill".to_owned(),
            Vec::new(),
        ))
        .expect("project");
    store
        .upsert_logical_resources(std::slice::from_ref(&source))
        .expect("source ownership");
    store
        .insert_credential_if_absent(&source_credential)
        .expect("source credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator");
    store
        .replace_managed_environment(&source_environment)
        .expect("source environment");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    drop(store);

    let result = runtime.block_on(execute_queued_project_restore(
        engine.clone(),
        FixedRestoreEntropy(0x44),
        ProjectRestoreExecutionOptions {
            operation: QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
                operation_id: "restore-42".to_owned(),
                recovery_point_id: "backup-42".to_owned(),
                project_id: "bill".to_owned(),
                service_id: "database".to_owned(),
                logical_resource_id: source.logical_resource_id().to_owned(),
                kind: source.kind().to_owned(),
                compatibility_fingerprint: fingerprint.clone(),
            })
            .expect("restore intent"),
            target: ProjectRestoreTargetPlan::Shared(shared.clone()),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 40_002,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(
        result.outcome(),
        &Ok(MigrationExecutionResult::AwaitingConfirmation)
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert_eq!(
        store.migrations().expect("migrations")[0].phase(),
        MigrationPhase::Cutover
    );
    assert_eq!(
        store.managed_environments().expect("environment")[0]
            .values()
            .get("DB_HOST"),
        Some(&"stackctl-migration-restore-42".to_owned())
    );
    assert!(
        engine
            .created()
            .contains(&"stackctl-migration-restore-42".to_owned())
    );
    assert!(engine.stopped().is_empty());
    assert!(engine.removed().is_empty());

    drop(store);
    let rollback_database_path = root.join("rollback-state.sqlite3");
    std::fs::copy(&database_path, &rollback_database_path)
        .expect("snapshot cutover state for independent rollback path");
    let decision = runtime.block_on(execute_queued_migration_decision(
        engine.clone(),
        FixedRestoreEntropy(0x55),
        MigrationDecisionExecutionOptions {
            operation: super::QueuedMigrationDecision::new(
                "confirm-42".to_owned(),
                "restore-42".to_owned(),
                "bill".to_owned(),
                IpcMigrationDecision::Confirm,
            )
            .expect("confirmation decision"),
            shared: shared.clone(),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 40_003,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(decision.outcome(), &Ok(MigrationExecutionResult::Confirmed));
    let store = SqliteStateStore::open(&database_path).expect("reopen confirmed state");
    assert_eq!(
        store.migrations().expect("confirmed migration")[0].phase(),
        MigrationPhase::Confirmed
    );
    assert_eq!(
        engine
            .command_environments()
            .last()
            .expect("source retirement command")
            .get("PGPASSWORD"),
        Some(&"source-admin".to_owned())
    );

    drop(store);
    let rollback = runtime.block_on(execute_queued_migration_decision(
        engine.clone(),
        FixedRestoreEntropy(0x66),
        MigrationDecisionExecutionOptions {
            operation: super::QueuedMigrationDecision::new(
                "rollback-42".to_owned(),
                "restore-42".to_owned(),
                "bill".to_owned(),
                IpcMigrationDecision::Rollback,
            )
            .expect("rollback decision"),
            shared,
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: rollback_database_path.clone(),
            backup_root,
            updated_at_unix_seconds: 40_004,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(
        rollback.outcome(),
        &Ok(MigrationExecutionResult::RolledBack)
    );
    let store = SqliteStateStore::open(&rollback_database_path).expect("reopen rolled-back state");
    let rolled_back = store
        .migrations()
        .expect("rolled-back migrations")
        .into_iter()
        .find(|migration| migration.migration_id() == "restore-42")
        .expect("rolled-back migration");
    assert_eq!(rolled_back.phase(), MigrationPhase::RolledBack);
    assert_eq!(
        store.managed_environments().expect("source environment")[0]
            .values()
            .get("DB_HOST"),
        Some(&source_container_name)
    );
    assert!(
        engine
            .created()
            .contains(&"stackctl-migration-restore-42".to_owned())
    );
    assert!(engine.removed().is_empty());

    drop(store);
    std::fs::remove_dir_all(root).expect("remove restore fixture");
}

#[test]
fn queued_mysql_restore_reconciles_isolated_target_and_reaches_reversible_cutover() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-mysql-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let image = concat!(
        "mysql@sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    let registry = plan_project_registry(&[ProjectSource::new(
        project_path.clone(),
        project_path.join(".stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  database:\n    preset: mysql\n    version: \"8\"\n    image: {image}\n"
        ),
    )])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared instances")
        .pop()
        .expect("MySQL instance");
    let fingerprint = shared.fingerprint().as_str().to_owned();
    let source_container_name = format!(
        "stackctl-shared-{}",
        fingerprint.strip_prefix("sha256:").expect("fingerprint")
    );
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        &source_container_name,
        "install-1",
        "mysql-source",
        &fingerprint,
    )]);
    let source = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: source_container_name.clone(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mysql_database".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:source".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let source_credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mysql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let fingerprint_id = fingerprint.strip_prefix("sha256:").expect("fingerprint");
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{fingerprint_id}/mysql-bootstrap"),
        project_id: None,
        service_id: "mysql".to_owned(),
        username: "root".to_owned(),
        secret: "source-admin".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let source_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:source".to_owned(),
        values: BTreeMap::from([
            ("DB_CONNECTION".to_owned(), "mysql".to_owned()),
            ("DB_HOST".to_owned(), source_container_name.clone()),
            ("DB_PORT".to_owned(), "3306".to_owned()),
            (
                "DB_DATABASE".to_owned(),
                "stackctl_bill_database".to_owned(),
            ),
            (
                "DB_USERNAME".to_owned(),
                source_credential.username().to_owned(),
            ),
            (
                "DB_PASSWORD".to_owned(),
                source_credential.secret().to_owned(),
            ),
        ]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let backup_root = root.join("backups");
    let backup = runtime
        .block_on(execute_queued_project_backup(
            engine.clone(),
            ProjectBackupExecutionOptions {
                operation: QueuedProjectBackup::new(
                    "backup-mysql".to_owned(),
                    "bill".to_owned(),
                    "database".to_owned(),
                    source.logical_resource_id().to_owned(),
                    source.kind().to_owned(),
                    fingerprint.clone(),
                )
                .expect("backup intent"),
                logical_resource: Ok(source.clone()),
                credential: Ok(source_credential.clone()),
                administrator: Ok(None),
                physical_resource: Ok(None),
                installation_id: "install-1".to_owned(),
                schema_version: 8,
                backup_root: backup_root.clone(),
                created_at_unix_seconds: 50_000,
                timeout: Duration::from_secs(30),
            },
        ))
        .outcome()
        .as_ref()
        .expect("verified backup")
        .clone();
    let recovery_point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-mysql".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: source.logical_resource_id().to_owned(),
        resource_kind: source.kind().to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        reference: backup.reference().to_owned(),
        artifact_sha256: backup.artifact_sha256().to_owned(),
        artifact_size_bytes: backup.artifact_size_bytes(),
        created_at_unix_seconds: 50_000,
        verified_at_unix_seconds: 50_001,
    })
    .expect("recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .replace_project(&ProjectRecord::new(
            project_path,
            "bill".to_owned(),
            Vec::new(),
        ))
        .expect("project");
    store
        .upsert_logical_resources(std::slice::from_ref(&source))
        .expect("source ownership");
    store
        .insert_credential_if_absent(&source_credential)
        .expect("source credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator");
    store
        .replace_managed_environment(&source_environment)
        .expect("source environment");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    drop(store);

    let result = runtime.block_on(execute_queued_project_restore(
        engine.clone(),
        FixedRestoreEntropy(0x77),
        ProjectRestoreExecutionOptions {
            operation: QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
                operation_id: "restore-mysql".to_owned(),
                recovery_point_id: "backup-mysql".to_owned(),
                project_id: "bill".to_owned(),
                service_id: "database".to_owned(),
                logical_resource_id: source.logical_resource_id().to_owned(),
                kind: source.kind().to_owned(),
                compatibility_fingerprint: fingerprint,
            })
            .expect("restore intent"),
            target: ProjectRestoreTargetPlan::Shared(shared.clone()),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 50_002,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(
        result.outcome(),
        &Ok(MigrationExecutionResult::AwaitingConfirmation)
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert_eq!(
        store.migrations().expect("migrations")[0].phase(),
        MigrationPhase::Cutover
    );
    assert_eq!(
        store.managed_environments().expect("environment")[0]
            .values()
            .get("DB_HOST"),
        Some(&"stackctl-migration-restore-mysql".to_owned())
    );
    assert!(
        engine
            .created()
            .contains(&"stackctl-migration-restore-mysql".to_owned())
    );
    assert!(engine.stopped().is_empty());
    assert!(engine.removed().is_empty());

    drop(store);
    let rollback_database_path = root.join("rollback-state.sqlite3");
    std::fs::copy(&database_path, &rollback_database_path)
        .expect("snapshot MySQL cutover state for rollback");
    let confirmed = runtime.block_on(execute_queued_migration_decision(
        engine.clone(),
        FixedRestoreEntropy(0x88),
        MigrationDecisionExecutionOptions {
            operation: super::QueuedMigrationDecision::new(
                "confirm-mysql".to_owned(),
                "restore-mysql".to_owned(),
                "bill".to_owned(),
                IpcMigrationDecision::Confirm,
            )
            .expect("MySQL confirmation"),
            shared: shared.clone(),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 50_003,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(
        confirmed.outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    let store = SqliteStateStore::open(&database_path).expect("confirmed MySQL state");
    assert_eq!(
        store.migrations().expect("confirmed migration")[0].phase(),
        MigrationPhase::Confirmed
    );
    assert_eq!(
        engine
            .command_environments()
            .last()
            .expect("MySQL retirement command")
            .get("MYSQL_PWD"),
        Some(&"source-admin".to_owned())
    );
    let retirement_sql = String::from_utf8(
        engine
            .command_inputs()
            .last()
            .expect("MySQL retirement SQL")
            .clone(),
    )
    .expect("retirement SQL UTF-8");
    assert!(retirement_sql.contains("DROP DATABASE IF EXISTS `stackctl_bill_database`"));
    assert!(retirement_sql.contains("DROP USER IF EXISTS 'st_bill_database'@'%'"));

    drop(store);
    let rolled_back = runtime.block_on(execute_queued_migration_decision(
        engine.clone(),
        FixedRestoreEntropy(0x99),
        MigrationDecisionExecutionOptions {
            operation: super::QueuedMigrationDecision::new(
                "rollback-mysql".to_owned(),
                "restore-mysql".to_owned(),
                "bill".to_owned(),
                IpcMigrationDecision::Rollback,
            )
            .expect("MySQL rollback"),
            shared,
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: rollback_database_path.clone(),
            backup_root,
            updated_at_unix_seconds: 50_004,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(
        rolled_back.outcome(),
        &Ok(MigrationExecutionResult::RolledBack)
    );
    let store = SqliteStateStore::open(&rollback_database_path).expect("rolled-back MySQL state");
    assert_eq!(
        store.migrations().expect("rolled-back migration")[0].phase(),
        MigrationPhase::RolledBack
    );
    assert_eq!(
        store.managed_environments().expect("source environment")[0]
            .values()
            .get("DB_HOST"),
        Some(&source_container_name)
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove MySQL restore fixture");
}

#[test]
fn queued_mongodb_restore_reaches_tenant_verified_reversible_cutover() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-mongodb-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let image = concat!(
        "mongo@sha256:",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    );
    let registry = plan_project_registry(&[ProjectSource::new(
        project_path.clone(),
        project_path.join(".stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  database:\n    preset: mongodb\n    version: \"8\"\n    image: {image}\n"
        ),
    )])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared instances")
        .pop()
        .expect("MongoDB instance");
    let fingerprint = shared.fingerprint().as_str().to_owned();
    let fingerprint_id = fingerprint.strip_prefix("sha256:").expect("fingerprint");
    let source_container_name = format!("stackctl-shared-{fingerprint_id}");
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        &source_container_name,
        "install-1",
        "mongodb-source",
        &fingerprint,
    )]);
    let source = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: source_container_name.clone(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mongodb_database".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:source".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let source_credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mongodb".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{fingerprint_id}/mongodb-bootstrap"),
        project_id: None,
        service_id: "mongodb".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "source-admin".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let source_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:source".to_owned(),
        values: BTreeMap::from([
            ("MONGODB_HOST".to_owned(), source_container_name.clone()),
            ("MONGODB_PORT".to_owned(), "27017".to_owned()),
            (
                "MONGODB_DATABASE".to_owned(),
                source.logical_resource_id().to_owned(),
            ),
            (
                "MONGODB_USERNAME".to_owned(),
                source_credential.username().to_owned(),
            ),
            (
                "MONGODB_PASSWORD".to_owned(),
                source_credential.secret().to_owned(),
            ),
        ]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let backup_root = root.join("backups");
    let backup = runtime
        .block_on(execute_queued_project_backup(
            engine.clone(),
            ProjectBackupExecutionOptions {
                operation: QueuedProjectBackup::new(
                    "backup-mongodb".to_owned(),
                    "bill".to_owned(),
                    "database".to_owned(),
                    source.logical_resource_id().to_owned(),
                    source.kind().to_owned(),
                    fingerprint.clone(),
                )
                .expect("backup intent"),
                logical_resource: Ok(source.clone()),
                credential: Ok(source_credential.clone()),
                administrator: Ok(None),
                physical_resource: Ok(None),
                installation_id: "install-1".to_owned(),
                schema_version: 8,
                backup_root: backup_root.clone(),
                created_at_unix_seconds: 60_000,
                timeout: Duration::from_secs(30),
            },
        ))
        .outcome()
        .as_ref()
        .expect("verified backup")
        .clone();
    let recovery_point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-mongodb".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: source.logical_resource_id().to_owned(),
        resource_kind: source.kind().to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        reference: backup.reference().to_owned(),
        artifact_sha256: backup.artifact_sha256().to_owned(),
        artifact_size_bytes: backup.artifact_size_bytes(),
        created_at_unix_seconds: 60_000,
        verified_at_unix_seconds: 60_001,
    })
    .expect("recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .replace_project(&ProjectRecord::new(
            project_path,
            "bill".to_owned(),
            Vec::new(),
        ))
        .expect("project");
    store
        .upsert_logical_resources(std::slice::from_ref(&source))
        .expect("source ownership");
    store
        .insert_credential_if_absent(&source_credential)
        .expect("source credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator");
    store
        .replace_managed_environment(&source_environment)
        .expect("source environment");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    drop(store);

    let result = runtime.block_on(execute_queued_project_restore(
        engine.clone(),
        FixedRestoreEntropy(0xaa),
        ProjectRestoreExecutionOptions {
            operation: QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
                operation_id: "restore-mongodb".to_owned(),
                recovery_point_id: "backup-mongodb".to_owned(),
                project_id: "bill".to_owned(),
                service_id: "database".to_owned(),
                logical_resource_id: source.logical_resource_id().to_owned(),
                kind: source.kind().to_owned(),
                compatibility_fingerprint: fingerprint,
            })
            .expect("restore intent"),
            target: ProjectRestoreTargetPlan::Shared(shared.clone()),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 60_002,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(
        result.outcome(),
        &Ok(MigrationExecutionResult::AwaitingConfirmation)
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert_eq!(
        store.migrations().expect("migrations")[0].phase(),
        MigrationPhase::Cutover
    );
    assert_eq!(
        store.managed_environments().expect("environment")[0]
            .values()
            .get("MONGODB_HOST"),
        Some(&"stackctl-migration-restore-mongodb".to_owned())
    );
    assert!(
        engine
            .created()
            .contains(&"stackctl-migration-restore-mongodb".to_owned())
    );
    assert!(engine.stopped().is_empty());
    assert!(engine.removed().is_empty());

    drop(store);
    let rollback_database_path = root.join("rollback-state.sqlite3");
    std::fs::copy(&database_path, &rollback_database_path)
        .expect("snapshot MongoDB cutover state for rollback");
    let confirmed = runtime.block_on(execute_queued_migration_decision(
        engine.clone(),
        FixedRestoreEntropy(0xbb),
        MigrationDecisionExecutionOptions {
            operation: super::QueuedMigrationDecision::new(
                "confirm-mongodb".to_owned(),
                "restore-mongodb".to_owned(),
                "bill".to_owned(),
                IpcMigrationDecision::Confirm,
            )
            .expect("MongoDB confirmation"),
            shared: shared.clone(),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 60_003,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(
        confirmed.outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    let store = SqliteStateStore::open(&database_path).expect("confirmed MongoDB state");
    assert_eq!(
        store.migrations().expect("confirmed migration")[0].phase(),
        MigrationPhase::Confirmed
    );
    let retirement_script = String::from_utf8(
        engine
            .command_inputs()
            .last()
            .expect("MongoDB retirement script")
            .clone(),
    )
    .expect("retirement script UTF-8");
    assert!(retirement_script.contains("dropUser"));
    assert!(retirement_script.contains("dropDatabase"));
    assert!(retirement_script.contains("source-admin"));

    drop(store);
    let rolled_back = runtime.block_on(execute_queued_migration_decision(
        engine.clone(),
        FixedRestoreEntropy(0xcc),
        MigrationDecisionExecutionOptions {
            operation: super::QueuedMigrationDecision::new(
                "rollback-mongodb".to_owned(),
                "restore-mongodb".to_owned(),
                "bill".to_owned(),
                IpcMigrationDecision::Rollback,
            )
            .expect("MongoDB rollback"),
            shared,
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: rollback_database_path.clone(),
            backup_root,
            updated_at_unix_seconds: 60_004,
            timeout: Duration::from_secs(30),
        },
    ));

    assert_eq!(
        rolled_back.outcome(),
        &Ok(MigrationExecutionResult::RolledBack)
    );
    let store = SqliteStateStore::open(&rollback_database_path).expect("rolled-back MongoDB state");
    assert_eq!(
        store.migrations().expect("rolled-back migration")[0].phase(),
        MigrationPhase::RolledBack
    );
    assert_eq!(
        store.managed_environments().expect("source environment")[0]
            .values()
            .get("MONGODB_HOST"),
        Some(&source_container_name)
    );
    assert!(engine.removed().is_empty());

    drop(store);
    std::fs::remove_dir_all(root).expect("remove MongoDB restore fixture");
}

#[test]
fn queued_sql_server_restore_reaches_tenant_verified_reversible_cutover() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-sqlserver-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let image = concat!(
        "mcr.microsoft.com/mssql/server@sha256:",
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    );
    let registry = plan_project_registry(&[ProjectSource::new(
        project_path.clone(),
        project_path.join(".stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  database:\n    preset: sqlserver\n    version: \"2022\"\n    image: {image}\n    environment:\n      ACCEPT_EULA: \"Y\"\n"
        ),
    )])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/amd64")
        .expect("shared instances")
        .pop()
        .expect("SQL Server instance");
    let fingerprint = shared.fingerprint().as_str().to_owned();
    let fingerprint_id = fingerprint.strip_prefix("sha256:").expect("fingerprint");
    let source_container_name = format!("stackctl-shared-{fingerprint_id}");
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        &source_container_name,
        "install-1",
        "sqlserver-source",
        &fingerprint,
    )]);
    let source = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: source_container_name.clone(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "sqlserver_database".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:source".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let source_credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/sqlserver".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "ProjectSecret1".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{fingerprint_id}/sqlserver-bootstrap"),
        project_id: None,
        service_id: "sqlserver".to_owned(),
        username: "sa".to_owned(),
        secret: "AdministratorSecret1".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let source_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:source".to_owned(),
        values: BTreeMap::from([
            ("DB_CONNECTION".to_owned(), "sqlsrv".to_owned()),
            (
                "DB_DATABASE".to_owned(),
                source.logical_resource_id().to_owned(),
            ),
            ("DB_HOST".to_owned(), source_container_name.clone()),
            (
                "DB_PASSWORD".to_owned(),
                source_credential.secret().to_owned(),
            ),
            ("DB_PORT".to_owned(), "1433".to_owned()),
            (
                "DB_USERNAME".to_owned(),
                source_credential.username().to_owned(),
            ),
        ]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let backup_root = root.join("backups");
    let backup = runtime
        .block_on(execute_queued_project_backup(
            engine.clone(),
            ProjectBackupExecutionOptions {
                operation: QueuedProjectBackup::new(
                    "backup-sqlserver".to_owned(),
                    "bill".to_owned(),
                    "database".to_owned(),
                    source.logical_resource_id().to_owned(),
                    source.kind().to_owned(),
                    fingerprint.clone(),
                )
                .expect("backup intent"),
                logical_resource: Ok(source.clone()),
                credential: Ok(source_credential.clone()),
                administrator: Ok(None),
                physical_resource: Ok(None),
                installation_id: "install-1".to_owned(),
                schema_version: 8,
                backup_root: backup_root.clone(),
                created_at_unix_seconds: 70_000,
                timeout: Duration::from_secs(120),
            },
        ))
        .outcome()
        .as_ref()
        .expect("verified backup")
        .clone();
    let recovery_point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-sqlserver".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: source.logical_resource_id().to_owned(),
        resource_kind: source.kind().to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        reference: backup.reference().to_owned(),
        artifact_sha256: backup.artifact_sha256().to_owned(),
        artifact_size_bytes: backup.artifact_size_bytes(),
        created_at_unix_seconds: 70_000,
        verified_at_unix_seconds: 70_001,
    })
    .expect("recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .replace_project(&ProjectRecord::new(
            project_path,
            "bill".to_owned(),
            Vec::new(),
        ))
        .expect("project");
    store
        .upsert_logical_resources(std::slice::from_ref(&source))
        .expect("source ownership");
    store
        .insert_credential_if_absent(&source_credential)
        .expect("source credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator");
    store
        .replace_managed_environment(&source_environment)
        .expect("source environment");
    store
        .record_recovery_point(&recovery_point)
        .expect("recovery point");
    drop(store);

    let result = runtime.block_on(execute_queued_project_restore(
        engine.clone(),
        FixedRestoreEntropy(0xdd),
        ProjectRestoreExecutionOptions {
            operation: QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
                operation_id: "restore-sqlserver".to_owned(),
                recovery_point_id: "backup-sqlserver".to_owned(),
                project_id: "bill".to_owned(),
                service_id: "database".to_owned(),
                logical_resource_id: source.logical_resource_id().to_owned(),
                kind: source.kind().to_owned(),
                compatibility_fingerprint: fingerprint,
            })
            .expect("restore intent"),
            target: ProjectRestoreTargetPlan::Shared(shared.clone()),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 70_002,
            timeout: Duration::from_secs(120),
        },
    ));

    assert_eq!(
        result.outcome(),
        &Ok(MigrationExecutionResult::AwaitingConfirmation)
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    assert_eq!(
        store.migrations().expect("migrations")[0].phase(),
        MigrationPhase::Cutover
    );
    assert_eq!(
        store.managed_environments().expect("environment")[0]
            .values()
            .get("DB_HOST"),
        Some(&"stackctl-migration-restore-sqlserver".to_owned())
    );
    assert!(
        engine
            .created()
            .contains(&"stackctl-migration-restore-sqlserver".to_owned())
    );
    assert!(engine.removed().is_empty());

    drop(store);
    let rollback_database_path = root.join("rollback-state.sqlite3");
    std::fs::copy(&database_path, &rollback_database_path)
        .expect("snapshot SQL Server cutover state for rollback");
    let confirmed = runtime.block_on(execute_queued_migration_decision(
        engine.clone(),
        FixedRestoreEntropy(0xee),
        MigrationDecisionExecutionOptions {
            operation: super::QueuedMigrationDecision::new(
                "confirm-sqlserver".to_owned(),
                "restore-sqlserver".to_owned(),
                "bill".to_owned(),
                IpcMigrationDecision::Confirm,
            )
            .expect("SQL Server confirmation"),
            shared: shared.clone(),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: database_path.clone(),
            backup_root: backup_root.clone(),
            updated_at_unix_seconds: 70_003,
            timeout: Duration::from_secs(120),
        },
    ));

    assert_eq!(
        confirmed.outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    let store = SqliteStateStore::open(&database_path).expect("confirmed SQL Server state");
    assert_eq!(
        store.migrations().expect("confirmed migration")[0].phase(),
        MigrationPhase::Confirmed
    );
    let retirement_sql = String::from_utf8(
        engine
            .command_inputs()
            .last()
            .expect("SQL Server retirement SQL")
            .clone(),
    )
    .expect("retirement SQL UTF-8");
    assert!(retirement_sql.contains("DROP DATABASE [stackctl_bill_database]"));
    assert!(retirement_sql.contains("DROP LOGIN [st_bill_database]"));

    drop(store);
    let rolled_back = runtime.block_on(execute_queued_migration_decision(
        engine.clone(),
        FixedRestoreEntropy(0xff),
        MigrationDecisionExecutionOptions {
            operation: super::QueuedMigrationDecision::new(
                "rollback-sqlserver".to_owned(),
                "restore-sqlserver".to_owned(),
                "bill".to_owned(),
                IpcMigrationDecision::Rollback,
            )
            .expect("SQL Server rollback"),
            shared,
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            state_database_path: rollback_database_path.clone(),
            backup_root,
            updated_at_unix_seconds: 70_004,
            timeout: Duration::from_secs(120),
        },
    ));

    assert_eq!(
        rolled_back.outcome(),
        &Ok(MigrationExecutionResult::RolledBack)
    );
    let store = SqliteStateStore::open(&rollback_database_path).expect("rolled-back state");
    assert_eq!(
        store.migrations().expect("rolled-back migration")[0].phase(),
        MigrationPhase::RolledBack
    );
    assert_eq!(
        store.managed_environments().expect("source environment")[0]
            .values()
            .get("DB_HOST"),
        Some(&source_container_name)
    );
    assert!(engine.removed().is_empty());

    drop(store);
    std::fs::remove_dir_all(root).expect("remove SQL Server restore fixture");
}

#[test]
fn queued_redis_restore_records_one_safety_snapshot_and_replays_in_place() {
    use crate::control_plane::retention::{
        BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
    };
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-redis-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let image = concat!(
        "redis@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[ProjectSource::new(
        project_path,
        root.join("bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  cache:\n    preset: redis\n    version: \"8\"\n    image: {image}\n"
        ),
    )])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared instances")
        .pop()
        .expect("Redis instance");
    let fingerprint = shared.fingerprint().as_str().to_owned();
    let container_name = format!(
        "stackctl-shared-{}",
        fingerprint.strip_prefix("sha256:").expect("fingerprint")
    );
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        &container_name,
        "install-1",
        "redis-source",
        &fingerprint,
    )]);
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/cache/redis".to_owned(),
        shared_resource_id: container_name,
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        kind: "redis_acl_prefix".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.logical_resource_id().to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "cache".to_owned(),
        username: "st_bill_cache".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!(
            "shared/{}/redis-bootstrap",
            fingerprint.strip_prefix("sha256:").expect("fingerprint")
        ),
        project_id: None,
        service_id: "redis".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let backup_root = root.join("backups");
    let snapshot = concat!(
        r#"{"format":1,"created_at_unix_seconds":40000,"prefix_hex":"737461636b63746c3a62696c6c3a63616368653a","records":["#,
        r#"{"key_hex":"737461636b63746c3a62696c6c3a63616368653a666f6f","dump_hex":"0001ff","ttl_milliseconds":-1}]}"#,
    );
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let stored =
        store_backup_artifact_for_identity(&identity, snapshot.as_bytes(), 40_000, &backup_root)
            .expect("stored Redis snapshot");
    let evidence = verify_stored_backup_artifact(&stored, 40_001).expect("verified snapshot");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-redis".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        logical_resource_id: logical.logical_resource_id().to_owned(),
        resource_kind: logical.kind().to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: evidence.artifact_sha256().to_owned(),
        artifact_size_bytes: evidence.artifact_size_bytes(),
        created_at_unix_seconds: 40_000,
        verified_at_unix_seconds: 40_001,
    })
    .expect("Redis recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical ownership");
    store
        .insert_credential_if_absent(&credential)
        .expect("tenant credential");
    store
        .insert_credential_if_absent(&administrator)
        .expect("administrator");
    store
        .record_recovery_point(&recovery)
        .expect("recovery point");
    drop(store);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let execute = || {
        runtime.block_on(execute_queued_project_restore(
            engine.clone(),
            FixedRestoreEntropy(0xaa),
            ProjectRestoreExecutionOptions {
                operation: QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
                    operation_id: "restore-redis".to_owned(),
                    recovery_point_id: "backup-redis".to_owned(),
                    project_id: "bill".to_owned(),
                    service_id: "cache".to_owned(),
                    logical_resource_id: logical.logical_resource_id().to_owned(),
                    kind: logical.kind().to_owned(),
                    compatibility_fingerprint: fingerprint.clone(),
                })
                .expect("restore intent"),
                target: ProjectRestoreTargetPlan::Shared(shared.clone()),
                installation_id: "install-1".to_owned(),
                network_name: "stackctl".to_owned(),
                schema_version: 8,
                state_database_path: database_path.clone(),
                backup_root: backup_root.clone(),
                updated_at_unix_seconds: 40_002,
                timeout: Duration::from_secs(30),
            },
        ))
    };

    assert_eq!(
        execute().outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    let first_calls = engine.command_arguments();
    assert_eq!(first_calls.len(), 3);
    assert!(first_calls[0].iter().any(|value| value.contains("DUMP")));
    assert!(first_calls[1].iter().any(|value| value.contains("RESTORE")));
    assert!(first_calls[2].iter().any(|value| value.contains("RENAME")));
    assert!(!format!("{first_calls:?}").contains("administrator-secret"));
    assert!(
        engine.command_environments()[..3]
            .iter()
            .all(|environment| environment["REDISCLI_AUTH"] == "administrator-secret")
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    let points = store.recovery_points("bill").expect("recovery catalog");
    assert_eq!(points.len(), 2);
    let safety = points
        .iter()
        .find(|point| point.recovery_point_id() == "restore-redis-pre-restore")
        .expect("safety recovery point");
    assert_eq!(safety.created_at_unix_seconds(), 40_002);
    drop(store);

    assert_eq!(
        execute().outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    assert_eq!(engine.command_arguments().len(), 5);
    assert_eq!(
        engine
            .command_arguments()
            .iter()
            .filter(|arguments| arguments.iter().any(|value| value.contains("DUMP")))
            .count(),
        1
    );
    assert_eq!(
        SqliteStateStore::open(&database_path)
            .expect("reopen replayed state")
            .recovery_points("bill")
            .expect("replayed recovery catalog")
            .len(),
        2
    );
    assert!(engine.created().is_empty());
    assert!(engine.stopped().is_empty());
    assert!(engine.removed().is_empty());

    std::fs::remove_dir_all(root).expect("remove Redis restore fixture");
}

#[test]
fn queued_minio_restore_records_one_safety_snapshot_and_replays_in_place() {
    use crate::control_plane::retention::{
        BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
    };
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-minio-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let image = concat!(
        "minio/minio@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[ProjectSource::new(
        project_path,
        root.join("bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  files:\n    preset: minio\n    version: \"1\"\n    image: {image}\n"
        ),
    )])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared instances")
        .pop()
        .expect("MinIO instance");
    let fingerprint = shared.fingerprint().as_str().to_owned();
    let container_name = format!(
        "stackctl-shared-{}",
        fingerprint.strip_prefix("sha256:").expect("fingerprint")
    );
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        &container_name,
        "install-1",
        "minio-source",
        &fingerprint,
    )]);
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/files/object-store".to_owned(),
        shared_resource_id: container_name,
        project_id: "bill".to_owned(),
        service_id: "files".to_owned(),
        kind: "minio_bucket_policy".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.logical_resource_id().to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "files".to_owned(),
        username: "st_bill_files".to_owned(),
        secret: "minio-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let backup_root = root.join("backups");
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let stored = store_backup_artifact_for_identity(
        &identity,
        b"verified MinIO tar archive",
        50_000,
        &backup_root,
    )
    .expect("stored MinIO snapshot");
    let evidence = verify_stored_backup_artifact(&stored, 50_001).expect("verified snapshot");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-minio".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "files".to_owned(),
        logical_resource_id: logical.logical_resource_id().to_owned(),
        resource_kind: logical.kind().to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: evidence.artifact_sha256().to_owned(),
        artifact_size_bytes: evidence.artifact_size_bytes(),
        created_at_unix_seconds: 50_000,
        verified_at_unix_seconds: 50_001,
    })
    .expect("MinIO recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical ownership");
    store
        .insert_credential_if_absent(&credential)
        .expect("tenant credential");
    store
        .record_recovery_point(&recovery)
        .expect("recovery point");
    drop(store);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let execute = || {
        runtime.block_on(execute_queued_project_restore(
            engine.clone(),
            FixedRestoreEntropy(0xbb),
            ProjectRestoreExecutionOptions {
                operation: QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
                    operation_id: "restore-minio".to_owned(),
                    recovery_point_id: "backup-minio".to_owned(),
                    project_id: "bill".to_owned(),
                    service_id: "files".to_owned(),
                    logical_resource_id: logical.logical_resource_id().to_owned(),
                    kind: logical.kind().to_owned(),
                    compatibility_fingerprint: fingerprint.clone(),
                })
                .expect("restore intent"),
                target: ProjectRestoreTargetPlan::Shared(shared.clone()),
                installation_id: "install-1".to_owned(),
                network_name: "stackctl".to_owned(),
                schema_version: 8,
                state_database_path: database_path.clone(),
                backup_root: backup_root.clone(),
                updated_at_unix_seconds: 50_002,
                timeout: Duration::from_secs(30),
            },
        ))
    };

    assert_eq!(
        execute().outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    let first_calls = engine.command_arguments();
    assert_eq!(first_calls.len(), 3);
    assert!(first_calls[0][2].contains("version info"));
    assert!(first_calls[1][2].contains("tar -C"));
    assert!(first_calls[2][2].contains("mirror --overwrite --remove"));
    assert!(!format!("{first_calls:?}").contains("minio-secret"));
    assert!(
        engine.command_environments()[..3]
            .iter()
            .all(|environment| environment["STACKCTL_SECRET_KEY"] == "minio-secret")
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    let points = store.recovery_points("bill").expect("recovery catalog");
    assert_eq!(points.len(), 2);
    let safety = points
        .iter()
        .find(|point| point.recovery_point_id() == "restore-minio-pre-restore")
        .expect("safety recovery point");
    assert_eq!(safety.created_at_unix_seconds(), 50_002);
    drop(store);

    assert_eq!(
        execute().outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    assert_eq!(engine.command_arguments().len(), 4);
    assert_eq!(
        engine
            .command_arguments()
            .iter()
            .filter(|arguments| arguments[2].contains("version info"))
            .count(),
        1
    );
    assert_eq!(
        SqliteStateStore::open(&database_path)
            .expect("reopen replayed state")
            .recovery_points("bill")
            .expect("replayed recovery catalog")
            .len(),
        2
    );
    assert!(engine.created().is_empty());
    assert!(engine.stopped().is_empty());
    assert!(engine.removed().is_empty());

    std::fs::remove_dir_all(root).expect("remove MinIO restore fixture");
}

#[test]
fn queued_rabbitmq_restore_records_one_safety_snapshot_and_replays_in_place() {
    use crate::control_plane::retention::{
        BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
    };
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let root = temporary_directory("queued-rabbitmq-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let image = concat!(
        "rabbitmq@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[ProjectSource::new(
        project_path,
        root.join("bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  database:\n    preset: rabbitmq\n    version: \"4\"\n    image: {image}\n"
        ),
    )])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared instances")
        .pop()
        .expect("RabbitMQ instance");
    let fingerprint = shared.fingerprint().as_str().to_owned();
    let container_name = format!(
        "stackctl-shared-{}",
        fingerprint.strip_prefix("sha256:").expect("fingerprint")
    );
    let volume_name = format!("{container_name}-data");
    let engine = RecordingProjectCommandEngine::new(vec![observed_shared_service(
        &container_name,
        "install-1",
        "rabbitmq-source",
        &fingerprint,
    )])
    .with_observed_volumes(vec![observed_shared_volume(
        &volume_name,
        "install-1",
        "rabbitmq-source",
        &fingerprint,
    )])
    .with_observed_networks(vec![observed_global_network(
        "stackctl-network",
        "install-1",
    )])
    .with_rabbitmq_queue_output(b"jobs\t2\t2\ttrue\tclassic\n".to_vec())
    .with_volume_archive(rabbitmq_message_store_tar(b"current-message"));
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database/rabbitmq".to_owned(),
        shared_resource_id: volume_name,
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "rabbitmq_vhost_user".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.logical_resource_id().to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "rabbit-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let backup_root = root.join("backups");
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let selected_definitions = serde_json::json!({
        "vhosts": [{"name": "stackctl_bill_database"}],
        "queues": [{"name": "jobs", "vhost": "stackctl_bill_database"}]
    });
    let selected_message_store = rabbitmq_message_store_tar(b"selected-message");
    let selected_artifact = rabbitmq_recovery_artifact_bytes(
        &selected_definitions,
        BTreeMap::from([("jobs".to_owned(), 2)]),
        &selected_message_store,
    );
    let stored =
        store_backup_artifact_for_identity(&identity, &selected_artifact, 60_000, &backup_root)
            .expect("stored RabbitMQ recovery artifact");
    let evidence = verify_stored_backup_artifact(&stored, 60_001).expect("verified definitions");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-rabbitmq".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: logical.logical_resource_id().to_owned(),
        resource_kind: logical.kind().to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: evidence.artifact_sha256().to_owned(),
        artifact_size_bytes: evidence.artifact_size_bytes(),
        created_at_unix_seconds: 60_000,
        verified_at_unix_seconds: 60_001,
    })
    .expect("RabbitMQ recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical ownership");
    store
        .insert_credential_if_absent(&credential)
        .expect("tenant credential");
    store
        .record_recovery_point(&recovery)
        .expect("recovery point");
    drop(store);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let execute = || {
        runtime.block_on(execute_queued_project_restore(
            engine.clone(),
            FixedRestoreEntropy(0xcc),
            ProjectRestoreExecutionOptions {
                operation: QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
                    operation_id: "restore-rabbitmq".to_owned(),
                    recovery_point_id: "backup-rabbitmq".to_owned(),
                    project_id: "bill".to_owned(),
                    service_id: "database".to_owned(),
                    logical_resource_id: logical.logical_resource_id().to_owned(),
                    kind: logical.kind().to_owned(),
                    compatibility_fingerprint: fingerprint.clone(),
                })
                .expect("restore intent"),
                target: ProjectRestoreTargetPlan::Shared(shared.clone()),
                installation_id: "install-1".to_owned(),
                network_name: "stackctl".to_owned(),
                schema_version: 8,
                state_database_path: database_path.clone(),
                backup_root: backup_root.clone(),
                updated_at_unix_seconds: 60_002,
                timeout: Duration::from_secs(30),
            },
        ))
    };

    assert_eq!(
        execute().outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    let first_calls = engine.command_arguments();
    assert_eq!(first_calls.len(), 11);
    assert_eq!(first_calls[0][1], "suspend_listeners");
    assert_eq!(first_calls[1][1], "close_all_connections");
    assert_eq!(first_calls[2][1], "list_queues");
    assert!(first_calls[3][2].contains("export_definitions"));
    assert!(first_calls[4][2].contains("msg_stores/vhosts"));
    assert_eq!(first_calls[5][1], "list_vhosts");
    assert!(first_calls[6][2].contains("delete_vhost"));
    assert!(first_calls[6][2].contains("import_definitions"));
    assert!(first_calls[6][2].contains("set_permissions"));
    assert!(first_calls[7][2].contains("msg_stores/vhosts"));
    assert!(first_calls[8][2].contains("import_definitions"));
    assert_eq!(first_calls[9][1], "list_vhosts");
    assert_eq!(first_calls[10][1], "list_queues");
    assert!(!format!("{first_calls:?}").contains("rabbit-secret"));
    assert!(
        engine
            .command_inputs()
            .iter()
            .any(|input| input == &serde_json::to_vec(&selected_definitions).expect("definitions"))
    );
    assert_eq!(
        *engine
            .execution
            .volume_upload
            .lock()
            .expect("RabbitMQ message-store upload"),
        selected_message_store
    );
    assert_eq!(
        engine.volume_subpath_uploads(),
        ["mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P"]
    );
    let store = SqliteStateStore::open(&database_path).expect("reopen state");
    let points = store.recovery_points("bill").expect("recovery catalog");
    assert_eq!(points.len(), 2);
    assert!(
        points
            .iter()
            .any(|point| point.recovery_point_id() == "restore-rabbitmq-pre-restore")
    );
    drop(store);

    assert_eq!(
        execute().outcome(),
        &Ok(MigrationExecutionResult::Confirmed)
    );
    assert_eq!(engine.command_arguments().len(), 17);
    assert_eq!(
        engine
            .command_arguments()
            .iter()
            .filter(|arguments| arguments.get(1).is_some_and(|value| value == "list_queues"))
            .count(),
        3
    );
    assert_eq!(
        SqliteStateStore::open(&database_path)
            .expect("reopen replayed state")
            .recovery_points("bill")
            .expect("replayed recovery catalog")
            .len(),
        2
    );
    assert!(engine.created().is_empty());
    assert_eq!(
        engine.stopped(),
        [
            container_name.clone(),
            container_name.clone(),
            container_name.clone(),
        ]
    );
    assert_eq!(
        engine.lifecycle_started(),
        [
            container_name.clone(),
            container_name.clone(),
            container_name.clone(),
        ]
    );
    assert_eq!(
        engine.disconnected_networks(),
        ["stackctl-network", "stackctl-network"]
    );
    assert_eq!(
        engine.reconnected_networks(),
        [
            ("stackctl-network".to_owned(), container_name.clone()),
            ("stackctl-network".to_owned(), container_name.clone()),
        ]
    );
    assert!(engine.removed().is_empty());

    std::fs::write(stored.artifact_file(), b"corrupt selected recovery")
        .expect("corrupt selected recovery artifact");
    let failed = execute();
    assert!(
        failed
            .outcome()
            .as_ref()
            .expect_err("corrupt recovery must fail")
            .contains("verification failed")
    );
    assert_eq!(
        engine.disconnected_networks(),
        ["stackctl-network", "stackctl-network", "stackctl-network"]
    );
    assert_eq!(
        engine.reconnected_networks(),
        [
            ("stackctl-network".to_owned(), container_name.clone()),
            ("stackctl-network".to_owned(), container_name.clone()),
            ("stackctl-network".to_owned(), container_name),
        ]
    );

    std::fs::remove_dir_all(root).expect("remove RabbitMQ restore fixture");
}

struct FixedRestoreEntropy(u8);

impl CredentialEntropy for FixedRestoreEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        bytes.fill(self.0);

        Ok(())
    }
}

fn rabbitmq_recovery_artifact_bytes(
    definitions: &serde_json::Value,
    queue_messages: BTreeMap<String, u64>,
    message_store: &[u8],
) -> Vec<u8> {
    let header = serde_json::to_vec(&serde_json::json!({
        "format_version": 1,
        "vhost": "stackctl_bill_database",
        "relative_message_store_path":
            "mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P",
        "definitions": definitions,
        "queue_messages": queue_messages,
    }))
    .expect("RabbitMQ recovery header");
    let mut artifact = b"STRMQ001".to_vec();
    artifact.extend_from_slice(&(header.len() as u64).to_be_bytes());
    artifact.extend_from_slice(&header);
    artifact.extend_from_slice(message_store);

    artifact
}

fn rabbitmq_message_store_tar(message: &[u8]) -> Vec<u8> {
    let mut archive = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut archive);
        let mut directory = tar::Header::new_gnu();
        directory.set_entry_type(tar::EntryType::Directory);
        directory.set_mode(0o700);
        directory.set_size(0);
        directory.set_cksum();
        builder
            .append_data(&mut directory, "628Q7P", std::io::empty())
            .expect("RabbitMQ message-store directory");
        let mut marker = tar::Header::new_gnu();
        marker.set_entry_type(tar::EntryType::Regular);
        marker.set_mode(0o600);
        marker.set_size(message.len() as u64);
        marker.set_cksum();
        builder
            .append_data(&mut marker, "628Q7P/msg_store_persistent", message)
            .expect("RabbitMQ message-store file");
        builder.finish().expect("finish RabbitMQ message-store tar");
    }

    archive
}

#[test]
fn project_backup_queue_is_bounded_and_rejects_duplicate_operations() {
    let operation = QueuedProjectBackup::new(
        "backup-42".to_owned(),
        "bill".to_owned(),
        "database".to_owned(),
        "stackctl_bill_database".to_owned(),
        "postgres_database_and_role".to_owned(),
        "sha256:postgres-17".to_owned(),
    )
    .expect("valid backup intent");
    let mut queue = ProjectBackupQueue::new(1).expect("bounded queue");

    queue.enqueue(operation.clone()).expect("first operation");
    let duplicate = queue
        .enqueue(operation)
        .expect_err("duplicate operation must fail");

    assert!(duplicate.to_string().contains("already queued"));
}

#[test]
fn project_backup_queue_accepts_redis_and_valkey_prefixes() {
    for kind in ["redis_acl_prefix", "valkey_acl_prefix"] {
        QueuedProjectBackup::new(
            format!("backup-{kind}"),
            "bill".to_owned(),
            "cache".to_owned(),
            format!("bill/cache/{}", kind.trim_end_matches("_acl_prefix")),
            kind.to_owned(),
            format!("sha256:{}", "a".repeat(64)),
        )
        .expect("Redis-compatible backup intent");
    }
}

#[test]
fn project_restore_queue_accepts_redis_and_valkey_prefixes() {
    for kind in ["redis_acl_prefix", "valkey_acl_prefix"] {
        QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
            operation_id: format!("restore-{kind}"),
            recovery_point_id: format!("backup-{kind}"),
            project_id: "bill".to_owned(),
            service_id: "cache".to_owned(),
            logical_resource_id: format!("bill/cache/{}", kind.trim_end_matches("_acl_prefix")),
            kind: kind.to_owned(),
            compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        })
        .expect("Redis-compatible restore intent");
    }
}

#[test]
fn project_restore_queue_accepts_minio_buckets() {
    QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
        operation_id: "restore-minio".to_owned(),
        recovery_point_id: "backup-minio".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "files".to_owned(),
        logical_resource_id: "bill/files/object-store".to_owned(),
        kind: "minio_bucket_policy".to_owned(),
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
    })
    .expect("MinIO restore intent");
}

#[test]
fn project_restore_queue_accepts_rabbitmq_vhosts() {
    QueuedProjectRestore::new(super::QueuedProjectRestoreOptions {
        operation_id: "restore-rabbitmq".to_owned(),
        recovery_point_id: "backup-rabbitmq".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "queue".to_owned(),
        logical_resource_id: "bill/queue/rabbitmq".to_owned(),
        kind: "rabbitmq_vhost_user".to_owned(),
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
    })
    .expect("RabbitMQ restore intent");
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
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            "    composer_image: composer@sha256:",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n"
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
    assert!(plan.applications()[0].runtime_image().is_some());
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
    let dedicated = &plan.dedicated_services()[0];
    let service = dedicated.request();
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
    assert_eq!(dedicated.volume(), None);
    assert!(plan.gateway().routes().is_empty());
}

#[test]
fn steady_engine_plans_exclude_ephemeral_browser_services() {
    let application_image = concat!(
        "ghcr.io/acme/bill@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let browser_image = concat!(
        "selenium/standalone-chromium@sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  app:\n    image: {application_image}\n  browser:\n    preset: dusk\n    image: {browser_image}\n"
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
    .expect("steady Engine plan");

    assert_eq!(plan.applications().len(), 1);
    assert!(plan.dedicated_services().is_empty());
    assert!(plan.processes().is_empty());
    assert_eq!(plan.gateway().routes().len(), 1);
    assert_eq!(
        plan.gateway().routes()[0].domain(),
        "bill-app.stackctl.localhost"
    );
}

#[test]
fn dedicated_stateful_services_plan_one_retained_project_volume() {
    let image = concat!(
        "localstack/localstack@sha256:",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  aws:\n    preset: localstack\n    version: '4'\n    image: {image}\n"
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

    let dedicated = &plan.dedicated_services()[0];
    let volume = dedicated.volume().expect("retained volume");
    assert_eq!(volume.name(), "stackctl-bill-aws-data");
    assert_eq!(
        volume.metadata().retention(),
        crate::control_plane::engine::RetentionClass::Persistent
    );
    assert_eq!(volume.metadata().project_id(), Some("bill"));
    assert_eq!(volume.metadata().resource_id(), Some("aws"));
    assert_eq!(dedicated.request().volume_mounts().len(), 1);
    assert_eq!(
        dedicated.request().volume_mounts()[0].source(),
        volume.name()
    );
    assert_eq!(
        dedicated.request().volume_mounts()[0].target(),
        "/var/lib/localstack"
    );
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
    assert_eq!(plan.processes()[0].application_service(), "app");
    assert_eq!(plan.processes()[0].request().name(), "stackctl-bill-worker");
    assert_eq!(plan.processes()[0].request().image(), image);
    assert_eq!(
        plan.processes()[0].request().command(),
        ["php", "artisan", "queue:work", "--no-interaction"]
    );
    assert_eq!(
        plan.processes()[0].request().environment().get("APP_MODE"),
        Some(&"local".to_owned())
    );
    assert_eq!(
        plan.processes()[0]
            .request()
            .environment()
            .get("WORKER_MODE"),
        Some(&"steady".to_owned())
    );
    assert_eq!(plan.gateway().routes().len(), 1);
}

#[test]
fn complete_engine_plans_schedule_commands_inside_the_application_runtime() {
    let image = concat!(
        "ghcr.io/acme/bill@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  app:\n    image: {image}\n    environment:\n      APP_MODE: local\n  scheduler:\n    preset: scheduler\n    depends_on: [app]\n    environment:\n      SCHEDULE_MODE: steady\n"
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

    assert!(plan.processes().is_empty());
    assert_eq!(plan.scheduled_commands().len(), 1);
    let scheduler = &plan.scheduled_commands()[0];
    assert_eq!(scheduler.project_id(), "bill");
    assert_eq!(scheduler.service_id(), "scheduler");
    assert_eq!(scheduler.application_service(), "app");
    assert_eq!(
        scheduler.arguments(),
        ["php", "artisan", "schedule:run", "--no-interaction"]
    );
    assert_eq!(
        scheduler.environment().get("APP_MODE"),
        Some(&"local".to_owned())
    );
    assert_eq!(
        scheduler.environment().get("SCHEDULE_MODE"),
        Some(&"steady".to_owned())
    );
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
fn orphaned_dedicated_service_volumes_require_adoption_before_engine_planning() {
    let image = concat!(
        "localstack/localstack@sha256:",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    );
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  aws:\n    preset: localstack\n    version: '4'\n    image: {image}\n"
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let volume = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "stackctl-bill-aws-data".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:localstack-4".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:data-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    })
    .with_scope_id("aws");

    let error = plan_engine_reconciliation(EngineReconciliationPlanOptions {
        execution: &execution,
        prepared_shared_services: &[],
        shared_routes: &[],
        managed_environments: &[],
        durable_resources: &[volume],
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect_err("orphaned project volume");

    assert_eq!(
        error.to_string(),
        "project workload 'bill-aws' data volume is orphaned; run explicit project adoption before reconciliation"
    );
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
    assert!(scheduler.has_pending_change());

    assert_eq!(
        scheduler.take_due(start),
        Some(DiscoveryScanReason::Initial)
    );
    assert!(!scheduler.has_pending_change());

    scheduler.record_filesystem_event(start + Duration::from_secs(1));
    assert!(scheduler.has_pending_change());
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
    assert!(!scheduler.has_pending_change());

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
fn watched_root_scan_attaches_a_bounded_project_local_artifact_lock() {
    let root = temporary_directory("artifact-lock-discovery");
    std::fs::write(
        root.join(".stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    image: ghcr.io/stackctl/php:8.4\n",
    )
    .expect("project config");
    std::fs::write(
        root.join(".stackctl.lock.yaml"),
        concat!(
            "schema_version: 1\nimages:\n  app:\n",
            "    source: ghcr.io/stackctl/php:8.4\n",
            "    resolved: ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        ),
    )
    .expect("artifact lock");

    let report = discover_project_sources(
        std::slice::from_ref(&root),
        ProjectDiscoveryOptions::bounded_defaults(),
    )
    .expect("bounded discovery");
    let registry = plan_project_registry(report.sources()).expect("locked registry");

    assert!(report.issues().is_empty());
    assert_eq!(
        registry.projects()[0].service("app").expect("app").image(),
        Some(concat!(
            "ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ))
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
    assert!(diagnostics.contains("create a new `.stackctl.yaml`"));

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
    assert!(!engine_schedule.is_converged());
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
    engine_schedule.mark_converged();
    assert!(!engine_schedule.is_due());
    assert!(engine_schedule.is_converged());
    engine_schedule.request();
    assert!(engine_schedule.is_due());
    assert!(!engine_schedule.is_converged());
    engine_schedule.complete();
    assert!(!engine_schedule.is_converged());
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
    assert!(!engine_schedule.is_converged());
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 10_001,
    });
    let IpcOutcome::Success {
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
fn daemon_benchmark_snapshot_is_complete_typed_and_read_only() {
    let root = temporary_directory("ipc-benchmark-snapshot");
    let store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "benchmark-42",
        IpcPayload::BenchmarkSnapshot {
            require_converged: true,
        },
    );
    let snapshot = IpcBenchmarkSnapshot::new(
        10_000,
        Vec::new(),
        vec![
            IpcBenchmarkContainerMetrics::new(IpcBenchmarkContainerMetricsOptions {
                container_id: "container-gateway".to_owned(),
                resource_kind: "gateway".to_owned(),
                compatibility_fingerprint: "gateway-v1".to_owned(),
                compatibility_implementation: None,
                compatibility_major_version: None,
                project_id: None,
                resource_id: None,
                cpu_usage_basis_points: 125,
                memory_usage_bytes: 64 * 1_024 * 1_024,
                process_count: 8,
                network_received_bytes: 1_000,
                network_transmitted_bytes: 2_000,
                published_tcp_ports: vec![
                    IpcBenchmarkTcpPort::new("127.0.0.1".to_owned(), 443).expect("TCP port"),
                ],
            })
            .expect("container metrics"),
        ],
    )
    .expect("benchmark snapshot");
    let mut provider = FixedBenchmarkSnapshotProvider {
        snapshot: snapshot.clone(),
        requests: Vec::new(),
    };

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut IpcEventJournal::default(),
        project_commands: &mut ProjectCommandQueue::default(),
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut ProjectLogSessionRegistry::default(),
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: Some(&mut provider),
        image_reference_resolution: None,
        now_unix_seconds: 10_000,
    });

    assert_eq!(
        response,
        IpcResponse::success("benchmark-42", IpcResult::BenchmarkSnapshot { snapshot })
    );
    assert_eq!(provider.requests, vec![(Vec::new(), 10_000, true)]);
    assert!(
        control_plane
            .resources()
            .expect("unchanged resources")
            .is_empty()
    );

    drop(control_plane);
    std::fs::remove_dir_all(root).expect("remove benchmark fixture");
}

#[test]
fn benchmark_collection_samples_only_exact_current_installation_ownership() {
    let engine = RecordingProjectCommandEngine::new(vec![
        observed_project_application("container-app", "install-1", "bill", "app"),
        observed_project_application("foreign-app", "install-2", "shop", "app"),
    ]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let snapshot = runtime
        .block_on(collect_benchmark_snapshot(
            &engine,
            "install-1",
            8,
            (0..40).map(|index| format!("project-{index}")).collect(),
            10_000,
        ))
        .expect("complete benchmark snapshot");

    assert_eq!(snapshot.project_count(), 40);
    assert_eq!(snapshot.observed_at_unix_seconds(), 10_000);
    assert_eq!(snapshot.containers().len(), 1);
    let container = &snapshot.containers()[0];
    assert_eq!(container.container_id(), "container-app");
    assert_eq!(container.resource_kind(), "project_application");
    assert_eq!(container.project_id(), Some("bill"));
    assert_eq!(container.resource_id(), Some("app"));
    assert_eq!(container.cpu_usage_basis_points(), 125);
    assert_eq!(container.memory_usage_bytes(), 64 * 1_024 * 1_024);
    assert_eq!(container.process_count(), 8);
    assert_eq!(container.network_received_bytes(), 1_000);
    assert_eq!(container.network_transmitted_bytes(), 2_000);
    assert_eq!(container.published_tcp_ports().len(), 1);
    assert_eq!(container.published_tcp_ports()[0].host_port(), 443);
}

#[test]
fn daemon_resolves_exact_image_sources_through_its_selected_engine_boundary() {
    let root = temporary_directory("ipc-image-resolution");
    let store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let mut control_plane = ControlPlane::new(store);
    let references = BTreeMap::from([("app".to_owned(), "ghcr.io/stackctl/php:8.4".to_owned())]);
    let request = IpcRequest::new(
        "lock-42",
        IpcPayload::ResolveImageReferences {
            references: references.clone(),
        },
    );
    let mut resolver = RecordingImageReferenceResolution::default();
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: Some(&mut resolver),
        now_unix_seconds: 10_000,
    });

    assert_eq!(resolver.requests, vec![references]);
    assert_eq!(
        response,
        IpcResponse::success(
            "lock-42",
            IpcResult::ImageReferencesResolved {
                references: BTreeMap::from([(
                    "app".to_owned(),
                    concat!(
                        "ghcr.io/stackctl/php@sha256:",
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    )
                    .to_owned(),
                )]),
            },
        )
    );

    drop(control_plane);
    std::fs::remove_dir_all(&root).expect("remove IPC fixture");
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
            kind: "postgres_database_and_role".to_owned(),
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &resource_health,
        benchmark_snapshot: None,
        image_reference_resolution: None,
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
                        IpcResourceStatus::with_data_lifecycle(
                            "db".to_owned(),
                            "postgres_database_and_role".to_owned(),
                            IpcResourceLifecycle::Active,
                            IpcResourceHealth::Unknown,
                            Some(9_800),
                            true,
                            IpcDataLifecycle::LogicalResource,
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
    let application_volume = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "stackctl-bill-app-data".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:application-data".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:data".to_owned(),
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
        .upsert_resources(&[application, application_volume, shared_database])
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
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
fn daemon_reports_only_the_exact_projects_durable_migrations() {
    let root = temporary_directory("ipc-project-migrations");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    store
        .replace_project(&ProjectRecord::new(
            project_path.clone(),
            "bill".to_owned(),
            Vec::new(),
        ))
        .expect("register project");
    for (migration_id, project_id) in [
        ("migration-bill-postgres", "bill"),
        ("migration-other-postgres", "other"),
    ] {
        for phase in [
            MigrationPhase::Inventoried,
            MigrationPhase::BackupVerified,
            MigrationPhase::TargetProvisioned,
            MigrationPhase::DataRestored,
            MigrationPhase::TargetVerified,
            MigrationPhase::Cutover,
        ] {
            store
                .record_migration(
                    &MigrationRecord::new(MigrationRecordOptions {
                        migration_id: migration_id.to_owned(),
                        project_id: project_id.to_owned(),
                        source_revision: "sha256:source".to_owned(),
                        target_revision: "sha256:target".to_owned(),
                        source_compatibility_fingerprint: "sha256:postgres-16".to_owned(),
                        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
                        phase,
                        backup_reference: (phase >= MigrationPhase::BackupVerified)
                            .then(|| "/private/recovery-point".to_owned()),
                        backup_artifact_sha256: (phase >= MigrationPhase::BackupVerified)
                            .then(|| "sha256:backup".to_owned()),
                        backup_artifact_size_bytes: (phase >= MigrationPhase::BackupVerified)
                            .then_some(1_024),
                        target_resource_id: (phase >= MigrationPhase::TargetProvisioned)
                            .then(|| format!("postgres-{project_id}")),
                        rollback_reference: Some("source:retained-source".to_owned()),
                        updated_at_unix_seconds: 12_345,
                    })
                    .expect("migration checkpoint"),
                )
                .expect("record migration");
        }
    }
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "migration-status-42",
        IpcPayload::ProjectMigrations {
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 20_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "migration-status-42",
            IpcResult::ProjectMigrations {
                migrations: vec![IpcMigrationStatus::new(
                    "migration-bill-postgres".to_owned(),
                    "cutover".to_owned(),
                    true,
                    true,
                    12_345,
                )],
            },
        )
    );
    assert!(!format!("{response:?}").contains("/private/recovery-point"));
    assert!(!format!("{response:?}").contains("source:retained-source"));

    drop(control_plane);
    std::fs::remove_dir_all(root).expect("remove migration fixture");
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
        values: BTreeMap::from([
            ("DB_HOST".to_owned(), "postgres.internal".to_owned()),
            ("DB_PASSWORD".to_owned(), "secret-value".to_owned()),
        ]),
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
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
    assert_eq!(
        queued.plan().environment().get("DB_PASSWORD"),
        Some(&"secret-value".to_owned())
    );

    drop(control_plane);
    let persisted = SqliteStateStore::open(&database_path)
        .expect("reopen state store")
        .active_daemon_operations()
        .expect("load queued operation");
    assert_eq!(persisted.len(), 1);
    assert!(!persisted[0].payload_json().contains("secret-value"));
    assert!(!persisted[0].payload_json().contains("DB_PASSWORD"));
    std::fs::remove_dir_all(&root).expect("remove command fixture");
}

#[test]
fn daemon_project_backup_request_persists_only_exact_secret_free_identity() {
    let root = temporary_directory("ipc-project-backup");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let logical = crate::control_plane::state::LogicalResourceRecord::new(
        crate::control_plane::state::LogicalResourceRecordOptions {
            logical_resource_id: "stackctl_bill_database".to_owned(),
            shared_resource_id: "postgres-17".to_owned(),
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            kind: "postgres_database_and_role".to_owned(),
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            desired_revision: "sha256:desired".to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        },
    );
    let credential = crate::control_plane::state::CredentialRecord::new(
        crate::control_plane::state::CredentialRecordOptions {
            credential_id: "bill/database/primary".to_owned(),
            project_id: Some("bill".to_owned()),
            service_id: "database".to_owned(),
            username: "credential-user".to_owned(),
            secret: "secret-value".to_owned(),
            lifecycle: crate::control_plane::state::CredentialLifecycle::Active,
        },
    );
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store.replace_project(&project).expect("register project");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("persist credential");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "backup-42",
        IpcPayload::BackupProjectService {
            canonical_path: project_path,
            service: "database".to_owned(),
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_backups = ProjectBackupQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_backups: &mut project_backups,
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "backup-42",
            IpcResult::Accepted {
                operation_id: "backup-42".to_owned(),
            },
        )
    );
    let queued = project_backups.pop_front().expect("queued project backup");
    assert_eq!(queued.project_id(), "bill");
    assert_eq!(queued.service_id(), "database");
    assert_eq!(queued.logical_resource_id(), "stackctl_bill_database");

    drop(control_plane);
    let persisted = SqliteStateStore::open(&database_path)
        .expect("reopen state store")
        .active_daemon_operations()
        .expect("load backup operation");
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].kind(), "project_backup");
    assert!(!persisted[0].payload_json().contains("secret-value"));
    assert!(!persisted[0].payload_json().contains("credential-user"));

    std::fs::remove_dir_all(root).expect("remove backup fixture");
}

#[test]
fn daemon_project_volume_backup_persists_exact_owned_volume_identity() {
    let root = temporary_directory("ipc-project-volume-backup");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let volume = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "stackctl-bill-search-data".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:search-3".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("search");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store.replace_project(&project).expect("register project");
    store
        .upsert_resources(std::slice::from_ref(&volume))
        .expect("persist owned volume");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "backup-volume-42",
        IpcPayload::BackupProjectService {
            canonical_path: project_path.clone(),
            service: "search".to_owned(),
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_backups = ProjectBackupQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_backups: &mut project_backups,
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "backup-volume-42",
            IpcResult::Accepted {
                operation_id: "backup-volume-42".to_owned(),
            },
        )
    );
    let queued = project_backups.pop_front().expect("queued volume backup");
    assert_eq!(queued.project_id(), "bill");
    assert_eq!(queued.service_id(), "search");
    assert_eq!(queued.kind(), "volume");
    assert_eq!(queued.logical_resource_id(), volume.resource_id());

    drop(control_plane);
    let persisted = SqliteStateStore::open(&database_path)
        .expect("reopen state store")
        .active_daemon_operations()
        .expect("load backup operation");
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].kind(), "project_backup");
    assert_eq!(persisted[0].operation_id(), "backup-volume-42");
    assert!(
        persisted[0]
            .payload_json()
            .contains("stackctl-bill-search-data")
    );
    assert!(
        !persisted[0]
            .payload_json()
            .contains(project_path.to_string_lossy().as_ref())
    );

    std::fs::remove_dir_all(root).expect("remove volume backup fixture");
}

#[test]
fn daemon_postgres_prune_plan_is_exact_and_effect_free() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
        InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    };

    let root = temporary_directory("ipc-postgres-prune-plan");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "postgres-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/primary".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill".to_owned(),
        secret: "runtime-only-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let recovery_point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: root.join("backups/backup-42.dump").display().to_string(),
        artifact_sha256: "a".repeat(64),
        artifact_size_bytes: 42,
        created_at_unix_seconds: 39_000,
        verified_at_unix_seconds: 39_100,
    })
    .expect("recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("initialize installation");
    store.replace_project(&project).expect("register project");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("persist credential");
    store
        .record_recovery_point(&recovery_point)
        .expect("persist recovery point");
    store
        .orphan_project(&project_path, 40_000)
        .expect("orphan project");
    let logical_before = store
        .logical_resources()
        .expect("logical state before plan");
    let credentials_before = store.credentials().expect("credential state before plan");
    let recovery_before = store
        .recovery_points("bill")
        .expect("recovery state before plan");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "prune-plan-42",
        IpcPayload::PlanPostgresPrune {
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            recovery_point_id: "backup-42".to_owned(),
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
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_100,
    });

    let IpcOutcome::Success {
        result: IpcResult::PostgresPrunePlan { plan },
    } = response.outcome()
    else {
        panic!("expected PostgreSQL prune plan: {response:?}");
    };
    assert_eq!(plan.project_id(), "bill");
    assert_eq!(plan.service_id(), "database");
    assert_eq!(plan.logical_resource_id(), "stackctl_bill_database");
    assert_eq!(plan.shared_resource_id(), "postgres-17");
    assert_eq!(plan.recovery_point_id(), "backup-42");
    assert_eq!(plan.confirmation_token().len(), 64);
    let confirmation_token = plan.confirmation_token().to_owned();
    assert_eq!(
        control_plane
            .logical_resources()
            .expect("logical state after plan"),
        logical_before
    );
    assert_eq!(
        control_plane
            .credentials()
            .expect("credential state after plan"),
        credentials_before
    );
    assert_eq!(
        control_plane
            .recovery_points("bill")
            .expect("recovery state after plan"),
        recovery_before
    );

    let mut postgres_prunes = PostgresPruneQueue::default();
    let stale_request = IpcRequest::new(
        "prune-stale-42",
        IpcPayload::ExecutePostgresPrune {
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            recovery_point_id: "backup-42".to_owned(),
            confirmation_token: "b".repeat(64),
        },
    );
    let stale = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &stale_request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut postgres_prunes,
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_101,
    });
    assert!(matches!(stale.outcome(), IpcOutcome::Failure { .. }));
    assert_eq!(postgres_prunes.len(), 0);

    let execute_request = IpcRequest::new(
        "prune-execute-42",
        IpcPayload::ExecutePostgresPrune {
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            recovery_point_id: "backup-42".to_owned(),
            confirmation_token,
        },
    );
    let accepted = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &execute_request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut postgres_prunes,
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_102,
    });
    assert_eq!(
        accepted,
        IpcResponse::success(
            "prune-execute-42",
            IpcResult::Accepted {
                operation_id: "prune-execute-42".to_owned(),
            },
        )
    );
    assert_eq!(postgres_prunes.len(), 1);

    drop(control_plane);
    let operations = SqliteStateStore::open(&database_path)
        .expect("reopen prune queue")
        .active_daemon_operations()
        .expect("queued prune operation");
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].kind(), "postgres_prune");
    assert!(!operations[0].payload_json().contains("runtime-only-secret"));
    assert!(!operations[0].payload_json().contains("/backups/"));
    std::fs::remove_dir_all(root).expect("remove prune-plan fixture");
}

#[test]
fn installation_deletion_queues_one_durable_logical_prune_without_duplicates() {
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
        InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    };

    let root = temporary_directory("installation-delete-prune");
    let database_path = root.join("state.sqlite3");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "postgres-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/primary".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill".to_owned(),
        secret: "runtime-only-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let recovery = stored_logical_recovery(&root, &logical, "backup-42");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("initialize installation");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("persist credential");
    store
        .record_recovery_point(&recovery)
        .expect("persist recovery point");
    let mut control_plane = ControlPlane::new(store);
    let deletion = control_plane
        .plan_installation_deletion()
        .expect("deletion plan");
    let mut queue = PostgresPruneQueue::default();
    let mut journal = IpcEventJournal::default();
    assert_eq!(
        queue_next_installation_deletion_prune(
            &mut control_plane,
            &mut queue,
            &mut journal,
            39_999,
        )
        .expect("active installation remains untouched"),
        None
    );
    control_plane
        .begin_confirmed_installation_deletion(deletion.confirmation_token(), 40_000)
        .expect("freeze installation");

    let operation_id = queue_next_installation_deletion_prune(
        &mut control_plane,
        &mut queue,
        &mut journal,
        40_001,
    )
    .expect("queue next deletion prune")
    .expect("one remaining logical resource");

    assert_eq!(queue.len(), 1);
    assert!(operation_id.starts_with("installation-delete-"));
    assert_eq!(
        queue_next_installation_deletion_prune(
            &mut control_plane,
            &mut queue,
            &mut journal,
            40_002,
        )
        .expect("occupied queue remains unchanged"),
        None
    );
    let queued = queue.pop_front().expect("queued logical prune");
    assert_eq!(queued.logical_resource_id(), "stackctl_bill_database");
    assert_eq!(queued.recovery_point_id(), "backup-42");
    assert_eq!(
        queue_next_installation_deletion_prune(
            &mut control_plane,
            &mut queue,
            &mut journal,
            40_003,
        )
        .expect("durable duplicate check"),
        None
    );
    let operation = control_plane
        .daemon_operation(&operation_id)
        .expect("read durable deletion operation")
        .expect("durable deletion operation");
    assert_eq!(operation.operation_id(), operation_id);
    assert!(!operation.payload_json().contains("runtime-only-secret"));
    assert!(!operation.payload_json().contains(&recovery.reference()));
    let failed_json = serde_json::to_string(&IpcEventKind::Failed {
        code: "engine_unavailable".to_owned(),
        message: "retry explicitly".to_owned(),
    })
    .expect("failed event");
    let failed = control_plane
        .transition_daemon_operation(DaemonOperationTransitionOptions {
            operation_id: &operation_id,
            expected: DaemonOperationStatus::Queued,
            next: DaemonOperationStatus::Failed,
            updated_at_unix_seconds: 40_004,
            event_kind_json: Some(&failed_json),
            event_retention_limit: journal.capacity(),
        })
        .expect("terminalize failed deletion prune")
        .expect("failed event record");
    journal.append_record(failed).expect("append failed event");
    let status_request = IpcRequest::new(
        "failed-delete-status",
        IpcPayload::InstallationDeletionStatus,
    );
    let status_response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &status_request,
        event_journal: &mut journal,
        project_commands: &mut ProjectCommandQueue::default(),
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut queue,
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut ProjectLogSessionRegistry::default(),
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_005,
    });
    let IpcOutcome::Success {
        result: IpcResult::InstallationDeletionStatus { status },
    } = status_response.outcome()
    else {
        panic!("expected failed installation deletion status: {status_response:?}");
    };
    assert_eq!(status.failed_operation_id(), Some(operation_id.as_str()));
    assert_eq!(status.blocking_error(), None);
    assert_eq!(
        retry_failed_installation_deletion_prune(
            &mut control_plane,
            &mut queue,
            &mut journal,
            40_006,
        )
        .expect("retry exact failed prune"),
        Some(operation_id.clone())
    );
    assert_eq!(queue.len(), 1);
    assert_eq!(
        control_plane
            .daemon_operation(&operation_id)
            .expect("read retried operation")
            .expect("retried operation")
            .status(),
        DaemonOperationStatus::Queued
    );

    std::fs::remove_dir_all(root).expect("remove deletion prune fixture");
}

#[test]
fn frozen_iteration_blocks_stale_engine_reconciliation() {
    let iteration = super::DaemonIterationResult::new(None, None, None, true);

    assert!(iteration.installation_reconciliation_frozen());
}

#[test]
fn installation_deletion_finalizes_after_logical_and_durable_work_is_empty() {
    use crate::control_plane::state::{EngineProvider, InstallationLifecycle, InstallationRecord};

    let root = temporary_directory("installation-delete-finalize");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("initialize installation");
    let mut control_plane = ControlPlane::new(store);
    let deletion = control_plane
        .plan_installation_deletion()
        .expect("empty deletion plan");
    control_plane
        .begin_confirmed_installation_deletion(deletion.confirmation_token(), 40_000)
        .expect("freeze empty installation");
    let mut engine = RecordingProjectCommandEngine::new(Vec::new());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let accepted_json = serde_json::to_string(&IpcEventKind::Accepted).expect("accepted event");
    control_plane
        .enqueue_daemon_operation(
            &DaemonOperationRecord::new(DaemonOperationRecordOptions {
                operation_id: "still-running".to_owned(),
                kind: "postgres_prune".to_owned(),
                payload_json: "{\"logical_resource_id\":\"pending\"}".to_owned(),
                status: DaemonOperationStatus::Queued,
                created_at_unix_seconds: 40_001,
                updated_at_unix_seconds: 40_001,
            }),
            &accepted_json,
            256,
        )
        .expect("persist pending deletion work");

    assert!(
        !runtime
            .block_on(finalize_installation_deletion(
                &mut control_plane,
                &mut engine,
                8,
                40_001,
            ))
            .expect("pending durable work blocks finalization")
    );
    let failed_json = serde_json::to_string(&IpcEventKind::Failed {
        code: "test_terminal".to_owned(),
        message: "test terminal operation".to_owned(),
    })
    .expect("failed event");
    control_plane
        .transition_daemon_operation(DaemonOperationTransitionOptions {
            operation_id: "still-running",
            expected: DaemonOperationStatus::Queued,
            next: DaemonOperationStatus::Failed,
            updated_at_unix_seconds: 40_002,
            event_kind_json: Some(&failed_json),
            event_retention_limit: 256,
        })
        .expect("terminalize pending deletion work");

    assert!(
        runtime
            .block_on(finalize_installation_deletion(
                &mut control_plane,
                &mut engine,
                8,
                40_003,
            ))
            .expect("finalize installation deletion")
    );
    assert_eq!(
        control_plane
            .installation_lifecycle()
            .expect("terminal installation lifecycle"),
        Some(InstallationLifecycle::Deleted)
    );
    assert!(engine.removed().is_empty());

    std::fs::remove_dir_all(root).expect("remove finalization fixture");
}

#[test]
fn installation_deletion_finalizes_exact_recovery_authorized_volume() {
    use crate::control_plane::retention::{
        BackupResourceIdentity, store_backup_artifact_for_identity,
    };
    use crate::control_plane::state::{EngineProvider, InstallationLifecycle, InstallationRecord};
    use sha2::{Digest, Sha256};

    let root = temporary_directory("installation-deletion-volume-finalize");
    let database_path = root.join("state.sqlite3");
    let backup_root = root.join("backups");
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "stackctl-bill-search-data".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:search-3".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("search");
    let stored = store_backup_artifact_for_identity(
        &BackupResourceIdentity::from_resource(&resource),
        b"volume archive",
        40_000,
        &backup_root,
    )
    .expect("store volume backup");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "volume-backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "search".to_owned(),
        logical_resource_id: resource.resource_id().to_owned(),
        resource_kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: hex::encode(Sha256::digest(b"volume archive")),
        artifact_size_bytes: 14,
        created_at_unix_seconds: 40_000,
        verified_at_unix_seconds: 40_000,
    })
    .expect("volume recovery");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "unix:///engine.sock",
        ))
        .expect("installation identity");
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("persist project volume");
    store
        .record_recovery_point(&recovery)
        .expect("catalog volume recovery");
    let mut control_plane = ControlPlane::new(store);
    let plan = control_plane
        .plan_installation_deletion()
        .expect("volume deletion plan");
    let ipc_plan = crate::control_plane::daemon::ipc::IpcInstallationDeletionPlan::from(&plan);
    assert_eq!(ipc_plan.volume_deletions().len(), 1);
    assert_eq!(
        ipc_plan.volume_deletions()[0].resource_id(),
        resource.resource_id()
    );
    assert_eq!(
        ipc_plan.volume_deletions()[0].recovery_point_id(),
        recovery.recovery_point_id()
    );
    control_plane
        .begin_confirmed_installation_deletion(plan.confirmation_token(), 40_001)
        .expect("freeze volume deletion");
    let volume_metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::Volume,
            project_id: Some("bill".to_owned()),
            compatibility_fingerprint: "sha256:search-3".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("volume metadata")
    .with_resource_id("search")
    .expect("volume service identity");
    let mut engine = RecordingProjectCommandEngine::new(Vec::new()).with_observed_volumes(vec![
        crate::control_plane::engine::ObservedVolume::new(
            resource.resource_id(),
            volume_metadata.labels(),
        ),
    ]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    assert!(
        runtime
            .block_on(finalize_installation_deletion(
                &mut control_plane,
                &mut engine,
                8,
                40_002,
            ))
            .expect("finalize protected volume deletion")
    );
    assert_eq!(
        engine
            .execution
            .removed_volumes
            .lock()
            .expect("removed protected volumes")
            .as_slice(),
        [resource.resource_id()]
    );
    assert_eq!(
        control_plane
            .installation_lifecycle()
            .expect("deleted installation"),
        Some(InstallationLifecycle::Deleted)
    );

    std::fs::remove_dir_all(root).expect("remove volume deletion fixture");
}

#[test]
fn daemon_confirms_and_reports_installation_deletion_over_typed_ipc() {
    use crate::control_plane::daemon::ipc::IpcInstallationLifecycle;
    use crate::control_plane::state::{EngineProvider, InstallationRecord};

    let root = temporary_directory("installation-delete-ipc");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "/docker.sock",
        ))
        .expect("initialize installation");
    let mut control_plane = ControlPlane::new(store);
    let mut event_journal = IpcEventJournal::default();
    let mut commands = ProjectCommandQueue::default();
    let mut backups = ProjectBackupQueue::default();
    let mut prunes = PostgresPruneQueue::default();
    let mut restores = ProjectRestoreQueue::default();
    let mut decisions = MigrationDecisionQueue::default();
    let mut logs = ProjectLogSessionRegistry::default();
    let health = ResourceHealthRegistry::default();
    let plan_request = IpcRequest::new("delete-plan", IpcPayload::PlanInstallationDeletion);
    let plan_response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &plan_request,
        event_journal: &mut event_journal,
        project_commands: &mut commands,
        project_backups: &mut backups,
        postgres_prunes: &mut prunes,
        project_restores: &mut restores,
        migration_decisions: &mut decisions,
        project_logs: &mut logs,
        resource_health: &health,
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_000,
    });
    let IpcOutcome::Success {
        result: IpcResult::InstallationDeletionPlan { plan },
    } = plan_response.outcome()
    else {
        panic!("expected installation deletion plan: {plan_response:?}");
    };
    let execute_request = IpcRequest::new(
        "delete-execute",
        IpcPayload::ExecuteInstallationDeletion {
            confirmation_token: plan.confirmation_token().to_owned(),
        },
    );
    let execute_response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &execute_request,
        event_journal: &mut event_journal,
        project_commands: &mut commands,
        project_backups: &mut backups,
        postgres_prunes: &mut prunes,
        project_restores: &mut restores,
        migration_decisions: &mut decisions,
        project_logs: &mut logs,
        resource_health: &health,
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_001,
    });
    assert_eq!(
        execute_response,
        IpcResponse::success("delete-execute", IpcResult::InstallationDeletionStarted)
    );
    let status_request = IpcRequest::new("delete-status", IpcPayload::InstallationDeletionStatus);
    let status_response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &status_request,
        event_journal: &mut event_journal,
        project_commands: &mut commands,
        project_backups: &mut backups,
        postgres_prunes: &mut prunes,
        project_restores: &mut restores,
        migration_decisions: &mut decisions,
        project_logs: &mut logs,
        resource_health: &health,
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_002,
    });
    let IpcOutcome::Success {
        result: IpcResult::InstallationDeletionStatus { status },
    } = status_response.outcome()
    else {
        panic!("expected installation deletion status: {status_response:?}");
    };
    assert_eq!(status.lifecycle(), IpcInstallationLifecycle::Deleting);
    assert_eq!(status.remaining_logical_resources(), 0);
    assert!(status.active_operation_ids().is_empty());
    assert_eq!(status.failed_operation_id(), None);
    assert_eq!(status.blocking_error(), None);

    std::fs::remove_dir_all(root).expect("remove deletion IPC fixture");
}

#[test]
fn daemon_project_restore_request_persists_exact_secret_free_recovery_point() {
    let root = temporary_directory("ipc-project-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let logical = crate::control_plane::state::LogicalResourceRecord::new(
        crate::control_plane::state::LogicalResourceRecordOptions {
            logical_resource_id: "stackctl_bill_database".to_owned(),
            shared_resource_id: "postgres-17".to_owned(),
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            kind: "postgres_database_and_role".to_owned(),
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            desired_revision: "sha256:desired".to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        },
    );
    let recovery_point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: root.join("backups/backup-42.dump").display().to_string(),
        artifact_sha256: "a".repeat(64),
        artifact_size_bytes: 42,
        created_at_unix_seconds: 39_000,
        verified_at_unix_seconds: 39_100,
    })
    .expect("recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store.replace_project(&project).expect("register project");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");
    store
        .record_recovery_point(&recovery_point)
        .expect("persist recovery point");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "restore-42",
        IpcPayload::RestoreProjectService {
            canonical_path: project_path,
            recovery_point_id: "backup-42".to_owned(),
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_backups = ProjectBackupQueue::default();
    let mut project_restores = ProjectRestoreQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_backups: &mut project_backups,
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut project_restores,
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "restore-42",
            IpcResult::Accepted {
                operation_id: "restore-42".to_owned(),
            },
        )
    );
    let queued: QueuedProjectRestore = project_restores
        .pop_front()
        .expect("queued project restore");
    assert_eq!(queued.recovery_point_id(), "backup-42");
    assert_eq!(queued.project_id(), "bill");
    assert_eq!(queued.service_id(), "database");
    assert_eq!(queued.logical_resource_id(), "stackctl_bill_database");

    drop(control_plane);
    let persisted = SqliteStateStore::open(&database_path)
        .expect("reopen state store")
        .active_daemon_operations()
        .expect("load restore operation");
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].kind(), "project_restore");
    assert!(!persisted[0].payload_json().contains("backup-42.dump"));
    assert!(!persisted[0].payload_json().contains("artifact_sha256"));

    std::fs::remove_dir_all(root).expect("remove restore fixture");
}

#[test]
fn daemon_project_volume_restore_requires_exact_active_physical_ownership() {
    let root = temporary_directory("ipc-project-volume-restore");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "stackctl-bill-aws-data".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:localstack-4".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("aws");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "volume-backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "aws".to_owned(),
        logical_resource_id: resource.resource_id().to_owned(),
        resource_kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        reference: root.join("backups/volume-backup-42").display().to_string(),
        artifact_sha256: "a".repeat(64),
        artifact_size_bytes: 42,
        created_at_unix_seconds: 39_000,
        verified_at_unix_seconds: 39_100,
    })
    .expect("volume recovery point");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store.replace_project(&project).expect("register project");
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("persist volume resource");
    store
        .record_recovery_point(&recovery)
        .expect("persist volume recovery");
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "restore-volume-42",
        IpcPayload::RestoreProjectService {
            canonical_path: project_path,
            recovery_point_id: recovery.recovery_point_id().to_owned(),
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut project_commands = ProjectCommandQueue::default();
    let mut project_backups = ProjectBackupQueue::default();
    let mut project_restores = ProjectRestoreQueue::default();
    let mut project_logs = ProjectLogSessionRegistry::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut project_commands,
        project_backups: &mut project_backups,
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut project_restores,
        migration_decisions: &mut MigrationDecisionQueue::default(),
        project_logs: &mut project_logs,
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_000,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "restore-volume-42",
            IpcResult::Accepted {
                operation_id: "restore-volume-42".to_owned(),
            },
        )
    );
    let queued = project_restores.pop_front().expect("queued volume restore");
    assert_eq!(queued.recovery_point_id(), "volume-backup-42");
    assert_eq!(queued.project_id(), "bill");
    assert_eq!(queued.service_id(), "aws");
    assert_eq!(queued.logical_resource_id(), resource.resource_id());
    assert_eq!(queued.kind(), "volume");

    std::fs::remove_dir_all(root).expect("remove volume restore fixture");
}

#[test]
fn daemon_migration_decision_persists_one_exact_operator_choice() {
    let root = temporary_directory("ipc-migration-decision");
    let project_path = root.join("bill");
    std::fs::create_dir(&project_path).expect("project directory");
    let project = ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new());
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    store.replace_project(&project).expect("register project");
    for (index, phase) in [
        MigrationPhase::Inventoried,
        MigrationPhase::BackupVerified,
        MigrationPhase::TargetProvisioned,
        MigrationPhase::DataRestored,
        MigrationPhase::TargetVerified,
        MigrationPhase::Cutover,
    ]
    .into_iter()
    .enumerate()
    {
        let has_backup = phase != MigrationPhase::Inventoried;
        let has_target = matches!(
            phase,
            MigrationPhase::TargetProvisioned
                | MigrationPhase::DataRestored
                | MigrationPhase::TargetVerified
                | MigrationPhase::Cutover
        );
        let migration = MigrationRecord::new(MigrationRecordOptions {
            migration_id: "restore-42".to_owned(),
            project_id: "bill".to_owned(),
            source_revision: "sha256:source".to_owned(),
            target_revision: "sha256:target".to_owned(),
            source_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            phase,
            backup_reference: has_backup.then(|| "/state/backups/backup-42.dump".to_owned()),
            backup_artifact_sha256: has_backup.then(|| "a".repeat(64)),
            backup_artifact_size_bytes: has_backup.then_some(42),
            target_resource_id: has_target.then(|| "bill/database".to_owned()),
            rollback_reference: Some("postgres-17".to_owned()),
            updated_at_unix_seconds: 40_000 + i64::try_from(index).expect("phase index"),
        })
        .expect("migration checkpoint");
        store
            .record_migration(&migration)
            .expect("persist migration");
    }
    let mut control_plane = ControlPlane::new(store);
    let request = IpcRequest::new(
        "decision-42",
        IpcPayload::DecideProjectMigration {
            canonical_path: project_path,
            migration_id: "restore-42".to_owned(),
            decision: IpcMigrationDecision::Confirm,
        },
    );
    let mut event_journal = IpcEventJournal::default();
    let mut decisions = MigrationDecisionQueue::default();

    let response = dispatch_daemon_request(DaemonRequestDispatchOptions {
        control_plane: &mut control_plane,
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        request: &request,
        event_journal: &mut event_journal,
        project_commands: &mut ProjectCommandQueue::default(),
        project_backups: &mut ProjectBackupQueue::default(),
        postgres_prunes: &mut PostgresPruneQueue::default(),
        project_restores: &mut ProjectRestoreQueue::default(),
        migration_decisions: &mut decisions,
        project_logs: &mut ProjectLogSessionRegistry::default(),
        resource_health: &ResourceHealthRegistry::default(),
        benchmark_snapshot: None,
        image_reference_resolution: None,
        now_unix_seconds: 40_100,
    });

    assert_eq!(
        response,
        IpcResponse::success(
            "decision-42",
            IpcResult::Accepted {
                operation_id: "decision-42".to_owned(),
            },
        )
    );
    let queued = decisions.pop_front().expect("queued migration decision");
    assert_eq!(queued.migration_id(), "restore-42");
    assert_eq!(queued.project_id(), "bill");
    assert_eq!(queued.decision(), IpcMigrationDecision::Confirm);
    assert!(
        !queued
            .payload_json()
            .expect("decision payload")
            .contains("backup")
    );

    drop(control_plane);
    let persisted = SqliteStateStore::open(&database_path)
        .expect("reopen state store")
        .active_daemon_operations()
        .expect("load migration decision");
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].kind(), "migration_decision");

    std::fs::remove_dir_all(root).expect("remove migration decision fixture");
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
fn singleton_unix_runtime_drains_active_work_when_shutdown_is_requested() {
    use super::{
        ActiveProjectCommand, ActiveProjectLogSession, DaemonShutdownSignal,
        ProjectCommandExecutionResult, UnixDaemonRuntime, UnixDaemonRuntimeOptions,
    };
    use crate::control_plane::engine::EngineError;

    struct RequestedShutdown;

    impl DaemonShutdownSignal for RequestedShutdown {
        fn is_requested(&self) -> bool {
            true
        }
    }

    let root = std::env::temp_dir().join(format!(
        "s8x-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("shutdown runtime root");
    let socket_path = root.join("daemon.sock");
    let options = UnixDaemonRuntimeOptions {
        state_database_path: root.join("state.sqlite3"),
        lease_path: root.join("daemon.lock"),
        socket_path: socket_path.clone(),
        discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
        scheduler_options: DiscoverySchedulerOptions::new(
            Duration::from_millis(25),
            Duration::from_millis(250),
            Duration::from_secs(60),
        )
        .expect("scheduler options"),
        idle_poll_interval: Duration::from_millis(5),
    };
    let mut runtime = UnixDaemonRuntime::new(options, Instant::now()).expect("singleton runtime");
    let command = runtime.engine_runtime.spawn(async {
        tokio::task::yield_now().await;
        ProjectCommandExecutionResult::new(
            "shutdown-command".to_owned(),
            Err(EngineError::Backend {
                detail: "expected shutdown test failure".to_owned(),
            }),
        )
    });
    runtime.active_project_command = Some(ActiveProjectCommand::new(
        "shutdown-command".to_owned(),
        command,
    ));
    let (_log_sender, log_receiver) = tokio::sync::mpsc::channel(1);
    let log_task = runtime.engine_runtime.spawn(async {
        std::future::pending::<()>().await;

        Ok(())
    });
    runtime.active_project_logs.insert(
        "shutdown-logs".to_owned(),
        ActiveProjectLogSession::new(log_receiver, log_task),
    );

    runtime.run_until_shutdown(&RequestedShutdown);

    assert!(runtime.active_project_command.is_none());
    assert!(runtime.active_project_logs.is_empty());
    assert!(
        socket_path.exists(),
        "runtime remains owned until it is dropped"
    );
    drop(runtime);
    assert!(
        !socket_path.exists(),
        "graceful drop removes the IPC socket"
    );
    std::fs::remove_dir_all(root).expect("remove shutdown fixture");
}

#[cfg(unix)]
#[test]
fn daemon_watch_rejects_broken_localhost_resolution_before_writing_state() {
    use super::{UnixDaemonWatchOptions, run_unix_daemon_watch_with_resolver};
    use crate::control_plane::gateway::{GatewayError, LocalhostResolver};
    use std::net::{IpAddr, Ipv4Addr};

    struct NonLoopbackResolver;

    impl LocalhostResolver for NonLoopbackResolver {
        fn resolve(&self, _host: &str) -> Result<Vec<IpAddr>, GatewayError> {
            Ok(vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10))])
        }
    }

    let root = temporary_directory("broken-localhost-preflight");
    let runtime_directory = root.join("runtime");
    let options = UnixDaemonWatchOptions {
        runtime_directory: runtime_directory.clone(),
        watched_roots: vec![root.clone()],
        once: true,
        periodic_rescan: Duration::from_secs(30),
    };

    let error = run_unix_daemon_watch_with_resolver(&options, &NonLoopbackResolver)
        .expect_err("non-loopback .localhost resolution");

    assert!(
        error
            .to_string()
            .contains("non-loopback address 192.0.2.10")
    );
    assert!(!runtime_directory.exists());
    std::fs::remove_dir_all(root).expect("remove preflight fixture");
}

#[test]
fn managed_engine_events_are_polled_without_blocking_and_resume_after_close() {
    use super::{EngineEventObservation, EngineEventSubscription};
    use crate::control_plane::engine::{
        ContainerEvent, ContainerEventAction, ContainerEventCursor, ContainerEventSource,
        ContainerEventStream, ContainerId,
    };

    #[derive(Clone, Copy)]
    struct OneEventSource;

    impl ContainerEventSource for OneEventSource {
        fn stream_managed<'stream>(
            &'stream self,
            _installation_id: &'stream str,
            _cursor: ContainerEventCursor,
        ) -> ContainerEventStream<'stream> {
            Box::pin(futures_util::stream::iter([Ok(ContainerEvent::new(
                ContainerId::new("container-app"),
                ContainerEventAction::Died,
                2_000_000_000,
            ))]))
        }
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("event runtime");
    let now = Instant::now();
    let mut subscription =
        EngineEventSubscription::new("installation-1").expect("event subscription");

    assert_eq!(
        subscription.poll(&runtime, Some(OneEventSource), now),
        EngineEventObservation::Events { count: 1 }
    );
    assert_eq!(
        subscription.poll(&runtime, Some(OneEventSource), now),
        EngineEventObservation::Idle
    );
    assert_eq!(
        subscription.poll(&runtime, Some(OneEventSource), now + Duration::from_secs(1),),
        EngineEventObservation::Events { count: 1 }
    );
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

#[cfg(unix)]
#[test]
fn watched_root_scan_never_follows_a_symlinked_artifact_lock() {
    use std::os::unix::fs::symlink;

    let root = temporary_directory("symlink-artifact-lock");
    std::fs::write(
        root.join(".stackctl.yaml"),
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("project config");
    let target = root.join("outside-lock.yaml");
    std::fs::write(&target, "schema_version: 1\nimages: {}\n").expect("lock target");
    symlink(&target, root.join(".stackctl.lock.yaml")).expect("lock symlink");

    let report = discover_project_sources(
        std::slice::from_ref(&root),
        ProjectDiscoveryOptions::bounded_defaults(),
    )
    .expect("bounded discovery");

    assert_eq!(report.sources().len(), 1);
    assert_eq!(report.issues().len(), 1);
    assert!(report.issues()[0].to_string().contains("artifact lock"));
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

#[derive(Default)]
struct RecordingImageReferenceResolution {
    requests: Vec<BTreeMap<String, String>>,
}

struct FixedBenchmarkSnapshotProvider {
    snapshot: IpcBenchmarkSnapshot,
    requests: Vec<(Vec<String>, i64, bool)>,
}

impl BenchmarkSnapshotProvider for FixedBenchmarkSnapshotProvider {
    fn snapshot(
        &mut self,
        project_ids: Vec<String>,
        observed_at_unix_seconds: i64,
        require_converged: bool,
    ) -> Result<IpcBenchmarkSnapshot, String> {
        self.requests
            .push((project_ids, observed_at_unix_seconds, require_converged));

        Ok(self.snapshot.clone())
    }
}

impl ImageReferenceResolution for RecordingImageReferenceResolution {
    fn resolve(
        &mut self,
        references: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, String> {
        self.requests.push(references.clone());

        Ok(references
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    concat!(
                        "ghcr.io/stackctl/php@sha256:",
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    )
                    .to_owned(),
                )
            })
            .collect())
    }
}

fn remove_lock(lock_path: &Path) {
    if lock_path.exists() {
        std::fs::remove_file(lock_path).expect("remove singleton lease");
    }
}

#[derive(Clone)]
struct RecordingProjectCommandEngine {
    observed: Vec<crate::control_plane::engine::ObservedContainer>,
    observed_volumes: Vec<crate::control_plane::engine::ObservedVolume>,
    observed_networks: Vec<crate::control_plane::engine::ObservedNetwork>,
    execution: std::sync::Arc<RecordingProjectCommandExecution>,
}

#[derive(Default)]
struct RecordingProjectCommandExecution {
    started: std::sync::atomic::AtomicUsize,
    command_exit_code: std::sync::atomic::AtomicI64,
    containers: std::sync::Mutex<Vec<String>>,
    command_arguments: std::sync::Mutex<Vec<Vec<String>>>,
    command_environments: std::sync::Mutex<Vec<BTreeMap<String, String>>>,
    command_inputs: std::sync::Mutex<Vec<Vec<u8>>>,
    created: std::sync::Mutex<Vec<String>>,
    lifecycle_started: std::sync::Mutex<Vec<String>>,
    stopped: std::sync::Mutex<Vec<String>>,
    removed: std::sync::Mutex<Vec<String>>,
    created_volumes: std::sync::Mutex<Vec<String>>,
    removed_volumes: std::sync::Mutex<Vec<String>>,
    rabbitmq_queue_output: std::sync::Mutex<Vec<u8>>,
    volume_archive: std::sync::Mutex<Vec<u8>>,
    volume_upload: std::sync::Mutex<Vec<u8>>,
    volume_subpath_downloads: std::sync::Mutex<Vec<String>>,
    volume_subpath_uploads: std::sync::Mutex<Vec<String>>,
    disconnected_networks: std::sync::Mutex<Vec<String>>,
    reconnected_networks: std::sync::Mutex<Vec<(String, String)>>,
    network_isolated: std::sync::atomic::AtomicBool,
}

impl RecordingProjectCommandEngine {
    fn new(observed: Vec<crate::control_plane::engine::ObservedContainer>) -> Self {
        Self {
            observed,
            observed_volumes: Vec::new(),
            observed_networks: Vec::new(),
            execution: std::sync::Arc::new(RecordingProjectCommandExecution::default()),
        }
    }

    fn with_observed_networks(
        mut self,
        networks: Vec<crate::control_plane::engine::ObservedNetwork>,
    ) -> Self {
        self.observed_networks = networks;
        self
    }

    fn with_observed_volumes(
        mut self,
        volumes: Vec<crate::control_plane::engine::ObservedVolume>,
    ) -> Self {
        self.observed_volumes = volumes;
        self
    }

    fn with_volume_archive(self, archive: Vec<u8>) -> Self {
        *self
            .execution
            .volume_archive
            .lock()
            .expect("volume archive") = archive;
        self
    }

    fn with_command_exit_code(self, exit_code: i64) -> Self {
        self.execution
            .command_exit_code
            .store(exit_code, std::sync::atomic::Ordering::Relaxed);
        self
    }

    fn with_rabbitmq_queue_output(self, output: Vec<u8>) -> Self {
        *self
            .execution
            .rabbitmq_queue_output
            .lock()
            .expect("RabbitMQ queue output") = output;
        self
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

    fn command_environments(&self) -> Vec<BTreeMap<String, String>> {
        self.execution
            .command_environments
            .lock()
            .expect("command environments")
            .clone()
    }

    fn command_arguments(&self) -> Vec<Vec<String>> {
        self.execution
            .command_arguments
            .lock()
            .expect("command arguments")
            .clone()
    }

    fn command_inputs(&self) -> Vec<Vec<u8>> {
        self.execution
            .command_inputs
            .lock()
            .expect("command inputs")
            .clone()
    }

    fn created(&self) -> Vec<String> {
        self.execution.created.lock().expect("created").clone()
    }

    fn lifecycle_started(&self) -> Vec<String> {
        self.execution
            .lifecycle_started
            .lock()
            .expect("lifecycle started")
            .clone()
    }

    fn stopped(&self) -> Vec<String> {
        self.execution.stopped.lock().expect("stopped").clone()
    }

    fn removed(&self) -> Vec<String> {
        self.execution.removed.lock().expect("removed").clone()
    }

    fn volume_subpath_downloads(&self) -> Vec<String> {
        self.execution
            .volume_subpath_downloads
            .lock()
            .expect("volume subpath downloads")
            .clone()
    }

    fn disconnected_networks(&self) -> Vec<String> {
        self.execution
            .disconnected_networks
            .lock()
            .expect("disconnected networks")
            .clone()
    }

    fn reconnected_networks(&self) -> Vec<(String, String)> {
        self.execution
            .reconnected_networks
            .lock()
            .expect("reconnected networks")
            .clone()
    }

    fn volume_subpath_uploads(&self) -> Vec<String> {
        self.execution
            .volume_subpath_uploads
            .lock()
            .expect("volume subpath uploads")
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

impl crate::control_plane::engine::PublishedPortDiscovery for RecordingProjectCommandEngine {
    fn discover_published_tcp_ports(
        &self,
    ) -> crate::control_plane::engine::EngineFuture<
        '_,
        Vec<crate::control_plane::engine::PublishedPortBinding>,
    > {
        let bindings = self
            .observed
            .iter()
            .map(|container| {
                crate::control_plane::engine::PublishedPortBinding::new(
                    container.id().as_str(),
                    container.id().as_str(),
                    "127.0.0.1".parse().expect("loopback address"),
                    443,
                )
                .expect("published port")
            })
            .collect();
        Box::pin(async move { Ok(bindings) })
    }
}

impl crate::control_plane::engine::ResourceMetrics for RecordingProjectCommandEngine {
    fn sample_resources<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
    ) -> crate::control_plane::engine::EngineFuture<
        'operation,
        crate::control_plane::engine::ContainerResourceMetrics,
    > {
        Box::pin(async {
            Ok(crate::control_plane::engine::ContainerResourceMetrics::new(
                Some(125),
                Some(64 * 1_024 * 1_024),
                Some(8),
                Some(1_000),
                Some(2_000),
            ))
        })
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
        request: &'operation crate::control_plane::engine::CommandRequest,
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
        self.execution
            .command_environments
            .lock()
            .expect("command environments")
            .push(request.environment().clone());
        self.execution
            .command_arguments
            .lock()
            .expect("command arguments")
            .push(request.arguments().to_vec());
        let postgres_verification_output = request
            .arguments()
            .iter()
            .any(|argument| argument.starts_with("--command=SELECT current_database()"))
            .then(|| {
                let database = request
                    .arguments()
                    .iter()
                    .find_map(|argument| argument.strip_prefix("--dbname="))
                    .expect("verification database");
                let role = request
                    .arguments()
                    .iter()
                    .find_map(|argument| argument.strip_prefix("--username="))
                    .expect("verification role");

                format!("{database}\t{role}\t0\t0\n").into_bytes()
            });
        let mysql_verification_output = request
            .arguments()
            .iter()
            .any(|argument| argument.starts_with("--execute=SELECT CONCAT(DATABASE()"))
            .then(|| {
                let database = request
                    .arguments()
                    .iter()
                    .find_map(|argument| argument.strip_prefix("--database="))
                    .expect("MySQL verification database");
                let user = request
                    .arguments()
                    .iter()
                    .find_map(|argument| argument.strip_prefix("--user="))
                    .expect("MySQL verification user");

                format!("{database}\t{user}@%\n").into_bytes()
            });
        let mongodb_verification_output = request
            .arguments()
            .iter()
            .any(|argument| argument.contains("db.getName()"))
            .then(|| {
                let database = request
                    .environment()
                    .get("STACKCTL_MONGODB_DATABASE")
                    .expect("MongoDB verification database");

                format!("{database}\n1\n").into_bytes()
            });
        let sql_server_verification_output = request
            .arguments()
            .iter()
            .any(|argument| {
                argument == "SET NOCOUNT ON; SELECT DB_NAME() + CHAR(9) + SUSER_SNAME()"
            })
            .then(|| {
                let arguments = request.arguments();
                let database = arguments
                    .windows(2)
                    .find_map(|pair| (pair[0] == "-d").then_some(pair[1].as_str()))
                    .expect("SQL Server verification database");
                let username = arguments
                    .windows(2)
                    .find_map(|pair| (pair[0] == "-U").then_some(pair[1].as_str()))
                    .expect("SQL Server verification login");
                format!("{database}\t{username}\n").into_bytes()
            });
        let rabbitmq_list_output = match request.arguments().get(1).map(String::as_str) {
            Some("list_users") => Some(b"st_bill_database\nother_user\n".to_vec()),
            Some("list_vhosts") => Some(b"/\nstackctl_bill_database\nother_vhost\n".to_vec()),
            Some("list_queues") => Some(
                self.execution
                    .rabbitmq_queue_output
                    .lock()
                    .expect("RabbitMQ queue output")
                    .clone(),
            ),
            _ => None,
        };
        let rabbitmq_script_output = request.arguments().get(2).and_then(|script| {
            if script.contains("export_definitions") {
                Some(b"{\"vhosts\":[{\"name\":\"stackctl_bill_database\"}],\"queues\":[]}".to_vec())
            } else if script.contains("msg_stores/vhosts") {
                Some(b"mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P\n".to_vec())
            } else {
                None
            }
        });
        let minio_version_output = request
            .arguments()
            .get(2)
            .is_some_and(|script| script.contains("version info"))
            .then(|| b"{\"status\":\"success\",\"versioning\":{}}\n".to_vec());
        let redis_snapshot_output = request
            .arguments()
            .iter()
            .any(|argument| argument.contains("redis.call('DUMP'"))
            .then(|| {
                let arguments = request.arguments();
                let prefix = arguments
                    .get(arguments.len().saturating_sub(2))
                    .expect("Redis snapshot prefix");
                let created_at = arguments.last().expect("Redis snapshot timestamp");
                format!(
                    "{{\"format\":1,\"created_at_unix_seconds\":{created_at},\
                     \"prefix_hex\":\"{}\",\"records\":{{}}}}\n",
                    hex::encode(prefix)
                )
                .into_bytes()
            });
        let redis_prune_output = request
            .arguments()
            .iter()
            .any(|argument| argument == "DELUSER")
            .then(|| b"1\n".to_vec())
            .or_else(|| {
                request
                    .arguments()
                    .iter()
                    .any(|argument| argument.contains("redis.call('UNLINK'"))
                    .then(|| b"2\n".to_vec())
            });
        let verification_output = postgres_verification_output
            .or(mysql_verification_output)
            .or(mongodb_verification_output)
            .or(sql_server_verification_output)
            .or(rabbitmq_list_output)
            .or(rabbitmq_script_output)
            .or(minio_version_output)
            .or(redis_snapshot_output)
            .or(redis_prune_output);
        let container_id = container.id().clone();
        let execution = self.execution.clone();
        Box::pin(async move {
            let (writer, mut reader) = tokio::io::duplex(1024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut input)
                    .await
                    .expect("consume command input");
                execution
                    .command_inputs
                    .lock()
                    .expect("command inputs")
                    .push(input);
            });
            let output: crate::control_plane::engine::ContainerLogStream<'static> =
                Box::pin(futures_util::stream::iter([
                    Ok(crate::control_plane::engine::LogChunk::stdout(
                        verification_output.unwrap_or_else(|| b"installed\n".to_vec()),
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
        let exit_code = self
            .execution
            .command_exit_code
            .load(std::sync::atomic::Ordering::Relaxed);
        Box::pin(async move {
            tokio::task::yield_now().await;
            Ok(crate::control_plane::engine::CommandStatus::Exited(
                exit_code,
            ))
        })
    }
}

impl crate::control_plane::engine::ContainerLifecycle for RecordingProjectCommandEngine {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation crate::control_plane::engine::ContainerCreateOptions,
    ) -> crate::control_plane::engine::EngineFuture<
        'operation,
        crate::control_plane::engine::OwnedContainer,
    > {
        self.execution
            .created
            .lock()
            .expect("created")
            .push(options.name().to_owned());
        let observed = crate::control_plane::engine::ObservedContainer::new(
            crate::control_plane::engine::ContainerId::new(options.name()),
            options.metadata().labels(),
        );
        let owned = crate::control_plane::engine::reconstruct_owned_container(
            &observed,
            options.metadata().installation_id(),
            options.metadata().schema_version(),
        )
        .expect("created ownership");

        Box::pin(async move { Ok(owned) })
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        self.execution
            .lifecycle_started
            .lock()
            .expect("lifecycle started")
            .push(container.id().as_str().to_owned());
        Box::pin(async { Ok(()) })
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        self.execution
            .stopped
            .lock()
            .expect("stopped")
            .push(container.id().as_str().to_owned());
        Box::pin(async { Ok(()) })
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        self.execution
            .removed
            .lock()
            .expect("removed")
            .push(container.id().as_str().to_owned());
        Box::pin(async { Ok(()) })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
    ) -> crate::control_plane::engine::EngineFuture<
        'operation,
        crate::control_plane::engine::ContainerState,
    > {
        Box::pin(async { Ok(crate::control_plane::engine::ContainerState::Running) })
    }
}

impl crate::control_plane::engine::HealthObserver for RecordingProjectCommandEngine {
    fn observe_health<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ContainerHealth> {
        Box::pin(async { Ok(ContainerHealth::Healthy) })
    }
}

impl crate::control_plane::engine::VolumeDiscovery for RecordingProjectCommandEngine {
    fn discover_managed_volumes(
        &self,
    ) -> crate::control_plane::engine::EngineFuture<
        '_,
        Vec<crate::control_plane::engine::ObservedVolume>,
    > {
        let volumes = self.observed_volumes.clone();
        Box::pin(async move { Ok(volumes) })
    }
}

impl crate::control_plane::engine::ContainerVolumeArchive for RecordingProjectCommandEngine {
    fn download_volume_archive<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
        _volume: &'operation crate::control_plane::engine::OwnedVolume,
        output: &'operation mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        let archive = self
            .execution
            .volume_archive
            .lock()
            .expect("volume archive")
            .clone();
        Box::pin(async move {
            tokio::io::AsyncWriteExt::write_all(output, &archive)
                .await
                .map_err(|error| crate::control_plane::engine::EngineError::Backend {
                    detail: error.to_string(),
                })
        })
    }

    fn upload_volume_archive<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
        _volume: &'operation crate::control_plane::engine::OwnedVolume,
        archive: &'operation Path,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        let archive = archive.to_path_buf();
        let execution = self.execution.clone();
        Box::pin(async move {
            let bytes = tokio::fs::read(archive).await.map_err(|error| {
                crate::control_plane::engine::EngineError::Backend {
                    detail: error.to_string(),
                }
            })?;
            *execution
                .volume_upload
                .lock()
                .expect("record volume upload") = bytes;

            Ok(())
        })
    }

    fn download_volume_subpath_archive<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
        _volume: &'operation crate::control_plane::engine::OwnedVolume,
        relative_path: &'operation Path,
        output: &'operation mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        self.execution
            .volume_subpath_downloads
            .lock()
            .expect("record volume subpath download")
            .push(relative_path.display().to_string());
        let archive = self
            .execution
            .volume_archive
            .lock()
            .expect("volume subpath archive")
            .clone();
        Box::pin(async move {
            tokio::io::AsyncWriteExt::write_all(output, &archive)
                .await
                .map_err(|error| crate::control_plane::engine::EngineError::Backend {
                    detail: error.to_string(),
                })
        })
    }

    fn upload_volume_subpath_archive<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
        _volume: &'operation crate::control_plane::engine::OwnedVolume,
        relative_path: &'operation Path,
        mut archive: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        self.execution
            .volume_subpath_uploads
            .lock()
            .expect("record volume subpath upload")
            .push(relative_path.display().to_string());
        let execution = self.execution.clone();
        Box::pin(async move {
            let mut bytes = Vec::new();
            tokio::io::AsyncReadExt::read_to_end(&mut archive, &mut bytes)
                .await
                .map_err(|error| crate::control_plane::engine::EngineError::Backend {
                    detail: error.to_string(),
                })?;
            *execution
                .volume_upload
                .lock()
                .expect("record volume subpath upload") = bytes;

            Ok(())
        })
    }
}

impl crate::control_plane::engine::VolumeManager for RecordingProjectCommandEngine {
    fn create_volume<'operation>(
        &'operation mut self,
        options: &'operation crate::control_plane::engine::VolumeCreateOptions,
    ) -> crate::control_plane::engine::EngineFuture<
        'operation,
        crate::control_plane::engine::OwnedVolume,
    > {
        self.execution
            .created_volumes
            .lock()
            .expect("created volumes")
            .push(options.name().to_owned());
        let observed = crate::control_plane::engine::ObservedVolume::new(
            options.name(),
            options.metadata().labels(),
        );
        let owned = crate::control_plane::engine::reconstruct_owned_volume(
            &observed,
            options.metadata().installation_id(),
            options.metadata().schema_version(),
        )
        .expect("created volume ownership");

        Box::pin(async move { Ok(owned) })
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        volume: &'operation crate::control_plane::engine::OwnedVolume,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        self.execution
            .removed_volumes
            .lock()
            .expect("removed volumes")
            .push(volume.name().to_owned());
        Box::pin(async { Ok(()) })
    }
}

impl crate::control_plane::engine::NetworkDiscovery for RecordingProjectCommandEngine {
    fn discover_managed_networks(
        &self,
    ) -> crate::control_plane::engine::EngineFuture<
        '_,
        Vec<crate::control_plane::engine::ObservedNetwork>,
    > {
        let networks = self.observed_networks.clone();
        Box::pin(async move { Ok(networks) })
    }
}

impl crate::control_plane::engine::ContainerNetworkIsolation for RecordingProjectCommandEngine {
    fn disconnect_container_network<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
        network: &'operation crate::control_plane::engine::OwnedNetwork,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        if !self
            .execution
            .network_isolated
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            self.execution
                .disconnected_networks
                .lock()
                .expect("disconnected networks")
                .push(network.id().as_str().to_owned());
        }
        Box::pin(async { Ok(()) })
    }

    fn reconnect_container_network<'operation>(
        &'operation self,
        _container: &'operation crate::control_plane::engine::OwnedContainer,
        network: &'operation crate::control_plane::engine::OwnedNetwork,
        alias: &'operation str,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        if self
            .execution
            .network_isolated
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.execution
                .reconnected_networks
                .lock()
                .expect("reconnected networks")
                .push((network.id().as_str().to_owned(), alias.to_owned()));
        }
        Box::pin(async { Ok(()) })
    }
}

impl crate::control_plane::engine::NetworkManager for RecordingProjectCommandEngine {
    fn create_network<'operation>(
        &'operation mut self,
        options: &'operation crate::control_plane::engine::NetworkCreateOptions,
    ) -> crate::control_plane::engine::EngineFuture<
        'operation,
        crate::control_plane::engine::OwnedNetwork,
    > {
        let observed = crate::control_plane::engine::ObservedNetwork::new(
            crate::control_plane::engine::NetworkId::new(options.name()),
            options.metadata().labels(),
        );
        let owned = crate::control_plane::engine::reconstruct_owned_network(
            &observed,
            options.metadata().installation_id(),
            options.metadata().schema_version(),
        )
        .expect("created network ownership");

        Box::pin(async move { Ok(owned) })
    }

    fn remove_network<'operation>(
        &'operation mut self,
        _network: &'operation crate::control_plane::engine::OwnedNetwork,
    ) -> crate::control_plane::engine::EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }
}

fn queued_composer_command(
    operation_id: &str,
    project_id: &str,
    service_id: &str,
) -> QueuedProjectCommand {
    queued_composer_command_with_browser(operation_id, project_id, service_id, false)
}

fn queued_browser_command(
    operation_id: &str,
    project_id: &str,
    service_id: &str,
) -> QueuedProjectCommand {
    queued_composer_command_with_browser(operation_id, project_id, service_id, true)
}

fn queued_composer_command_with_browser(
    operation_id: &str,
    project_id: &str,
    service_id: &str,
    browser_session: bool,
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
            browser_session,
        },
    )
    .expect("project command plan");

    QueuedProjectCommand::new(operation_id.to_owned(), service_id.to_owned(), plan)
}

fn command_execution_options(operation: QueuedProjectCommand) -> ProjectCommandExecutionOptions {
    ProjectCommandExecutionOptions {
        operation,
        installation_id: "install-1".to_owned(),
        schema_version: 8,
        ephemeral_browser: None,
        managed_environment: Ok(BTreeMap::new()),
    }
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

fn observed_shared_service(
    container_id: &str,
    installation_id: &str,
    resource_id: &str,
    compatibility_fingerprint: &str,
) -> crate::control_plane::engine::ObservedContainer {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: installation_id.to_owned(),
            kind: crate::control_plane::engine::ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: compatibility_fingerprint.to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("shared-service metadata")
    .with_resource_id(resource_id)
    .expect("shared-service identity");

    crate::control_plane::engine::ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new(container_id),
        metadata.labels(),
    )
}

fn observed_shared_volume(
    volume_name: &str,
    installation_id: &str,
    resource_id: &str,
    compatibility_fingerprint: &str,
) -> crate::control_plane::engine::ObservedVolume {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: installation_id.to_owned(),
            kind: crate::control_plane::engine::ResourceKind::Volume,
            project_id: None,
            compatibility_fingerprint: compatibility_fingerprint.to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("shared-volume metadata")
    .with_resource_id(resource_id)
    .expect("shared-volume identity");

    crate::control_plane::engine::ObservedVolume::new(volume_name, metadata.labels())
}

fn observed_global_network(
    network_id: &str,
    installation_id: &str,
) -> crate::control_plane::engine::ObservedNetwork {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: installation_id.to_owned(),
            kind: crate::control_plane::engine::ResourceKind::Network,
            project_id: None,
            compatibility_fingerprint: "network-v1".to_owned(),
            schema_version: 8,
            desired_revision: "network-v1".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("network metadata")
    .with_resource_id("private")
    .expect("network identity");

    crate::control_plane::engine::ObservedNetwork::new(
        crate::control_plane::engine::NetworkId::new(network_id),
        metadata.labels(),
    )
}

fn observed_migration_target(
    container_id: &str,
    installation_id: &str,
    project_id: &str,
    migration_id: &str,
    compatibility_fingerprint: &str,
) -> crate::control_plane::engine::ObservedContainer {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: installation_id.to_owned(),
            kind: crate::control_plane::engine::ResourceKind::ProjectService,
            project_id: Some(project_id.to_owned()),
            compatibility_fingerprint: compatibility_fingerprint.to_owned(),
            schema_version: 8,
            desired_revision: "sha256:target".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("migration target metadata")
    .with_resource_id(migration_id)
    .expect("migration target identity");

    crate::control_plane::engine::ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new(container_id),
        metadata.labels(),
    )
}

fn postgres_prune_restart_fixture(
    operation_id: &str,
) -> (
    QueuedPostgresPrune,
    crate::control_plane::state::LogicalResourceRecord,
    crate::control_plane::state::CredentialRecord,
) {
    use crate::control_plane::retention::{
        PostgresLogicalPrunePlan, PostgresLogicalPrunePlanOptions,
    };
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
        LogicalResourceRecordOptions,
    };

    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "postgres-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/primary".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: "/backups/backup-42".to_owned(),
        artifact_sha256: "a".repeat(64),
        artifact_size_bytes: 42,
        created_at_unix_seconds: 39_000,
        verified_at_unix_seconds: 39_100,
    })
    .expect("recovery fixture");
    let plan = PostgresLogicalPrunePlan::new(PostgresLogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-42",
        project_registered: false,
        logical_resources: std::slice::from_ref(&logical),
        credentials: std::slice::from_ref(&credential),
        recovery_points: std::slice::from_ref(&recovery),
    })
    .expect("prune restart plan");
    let queued = QueuedPostgresPrune::new(
        operation_id.to_owned(),
        &plan,
        plan.confirmation_token().to_owned(),
    )
    .expect("queued restart prune");

    (queued, logical, credential)
}

fn stored_logical_recovery(
    root: &Path,
    logical: &crate::control_plane::state::LogicalResourceRecord,
    recovery_point_id: &str,
) -> RecoveryPointRecord {
    use crate::control_plane::retention::{
        BackupResourceIdentity, store_backup_artifact_for_identity,
    };
    use sha2::{Digest, Sha256};

    let artifact = format!("backup:{recovery_point_id}").into_bytes();
    let stored = store_backup_artifact_for_identity(
        &BackupResourceIdentity::from_logical(logical, "install-1"),
        &artifact,
        39_000,
        &root.join("backups"),
    )
    .expect("stored logical recovery");
    RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: recovery_point_id.to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        logical_resource_id: logical.logical_resource_id().to_owned(),
        resource_kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: hex::encode(Sha256::digest(&artifact)),
        artifact_size_bytes: u64::try_from(artifact.len()).expect("artifact size"),
        created_at_unix_seconds: 39_000,
        verified_at_unix_seconds: 39_100,
    })
    .expect("recovery point")
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
