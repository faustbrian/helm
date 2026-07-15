use super::{
    IPC_PROTOCOL_VERSION, IpcBenchmarkContainerMetrics, IpcBenchmarkContainerMetricsOptions,
    IpcBenchmarkSnapshot, IpcBenchmarkTcpPort, IpcDiagnostic, IpcEventJournal, IpcEventKind,
    IpcInstallationDeletionPlan, IpcInstallationDeletionStatus, IpcInstallationLifecycle,
    IpcLogChunk, IpcLogSessionState, IpcMigrationStatus, IpcNodePackageManager, IpcOutputStream,
    IpcPayload, IpcPostgresPrunePlan, IpcPostgresPrunePlanOptions, IpcProjectCommand, IpcRequest,
    IpcResourceHealth, IpcResourceLifecycle, IpcResourceStatus, IpcResponse, IpcResult,
    decode_request_frame, decode_response_frame, encode_frame,
};
use crate::control_plane::state::DaemonEventRecord;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn request_frames_round_trip_with_version_id_and_typed_payload() {
    let request = IpcRequest::new("request-42", IpcPayload::Reconcile);

    let frame = encode_frame(&request).expect("encode request");
    let decoded = decode_request_frame(&frame).expect("decode request");

    assert!(frame.ends_with(b"\n"));
    assert_eq!(decoded, request);
    assert_eq!(decoded.protocol_version(), IPC_PROTOCOL_VERSION);
    assert_eq!(decoded.request_id(), "request-42");
}

#[test]
fn gateway_certificate_activation_round_trips_the_exact_generation() {
    let generation = "a".repeat(64);
    let request = IpcRequest::new(
        "certificate-42",
        IpcPayload::ActivateGatewayCertificate {
            generation: generation.clone(),
        },
    );
    let response = IpcResponse::success(
        "certificate-42",
        IpcResult::GatewayCertificateActivationRequested { generation },
    );

    assert_eq!(
        decode_request_frame(&encode_frame(&request).expect("encode request"))
            .expect("decode request"),
        request
    );
    assert_eq!(
        decode_response_frame(&encode_frame(&response).expect("encode response"))
            .expect("decode response"),
        response
    );
}

#[test]
fn daemon_status_round_trips_structured_discovery_diagnostics() {
    let diagnostic = IpcDiagnostic::new(
        "configuration_invalid",
        "project config '/work/bill/.stackctl.yaml' has an unknown field",
        false,
    );
    let request = IpcRequest::new("status-42", IpcPayload::DaemonStatus);
    let response = IpcResponse::success(
        "status-42",
        IpcResult::DaemonStatus {
            discovery_diagnostics: vec![diagnostic.clone()],
        },
    );
    let event = IpcEventKind::Diagnostics {
        diagnostics: vec![diagnostic],
    };

    assert_eq!(
        decode_request_frame(&encode_frame(&request).expect("encode request"))
            .expect("decode request"),
        request
    );
    assert_eq!(
        decode_response_frame(&encode_frame(&response).expect("encode response"))
            .expect("decode response"),
        response
    );
    assert_eq!(
        serde_json::from_str::<IpcEventKind>(
            &serde_json::to_string(&event).expect("encode diagnostic event")
        )
        .expect("decode diagnostic event"),
        event
    );
}

#[test]
fn benchmark_snapshots_round_trip_complete_integer_metrics_and_owned_ports() {
    let request = IpcRequest::new(
        "benchmark-42",
        IpcPayload::BenchmarkSnapshot {
            require_converged: true,
        },
    );
    let snapshot = IpcBenchmarkSnapshot::new(
        10_000,
        (0..40).map(|index| format!("project-{index}")).collect(),
        vec![
            IpcBenchmarkContainerMetrics::new(IpcBenchmarkContainerMetricsOptions {
                container_id: "gateway-container".to_owned(),
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
                    IpcBenchmarkTcpPort::new("127.0.0.1".to_owned(), 443).expect("published port"),
                ],
            })
            .expect("container sample"),
        ],
    )
    .expect("snapshot");
    let response = IpcResponse::success(
        "benchmark-42",
        IpcResult::BenchmarkSnapshot {
            snapshot: snapshot.clone(),
        },
    );

    assert_eq!(
        decode_request_frame(&encode_frame(&request).expect("encode request"))
            .expect("decode request"),
        request
    );
    assert_eq!(
        decode_response_frame(&encode_frame(&response).expect("encode response"))
            .expect("decode response"),
        response
    );
    let json = serde_json::to_string(&snapshot).expect("benchmark JSON");
    assert!(json.contains("\"memory_usage_bytes\":67108864"));
    assert!(json.contains("\"compatibility_fingerprint\":\"gateway-v1\""));
    assert!(!json.contains("null"));
}

#[test]
fn postgres_prune_plans_round_trip_without_credentials_or_backup_paths() {
    use crate::control_plane::retention::DataLifecycleStrategy;

    let request = IpcRequest::new(
        "prune-plan-42",
        IpcPayload::PlanPostgresPrune {
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            recovery_point_id: "backup-42".to_owned(),
        },
    );
    let plan = IpcPostgresPrunePlan::new(IpcPostgresPrunePlanOptions {
        strategy: DataLifecycleStrategy::PostgreSqlLogical,
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        credential_id: "bill/database/postgresql".to_owned(),
        recovery_point_id: "backup-42".to_owned(),
        confirmation_token: "a".repeat(64),
    })
    .expect("IPC prune plan");
    let response = IpcResponse::success("prune-plan-42", IpcResult::PostgresPrunePlan { plan });
    let execute = IpcRequest::new(
        "prune-execute-42",
        IpcPayload::ExecutePostgresPrune {
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            recovery_point_id: "backup-42".to_owned(),
            confirmation_token: "a".repeat(64),
        },
    );

    assert_eq!(
        decode_request_frame(&encode_frame(&request).expect("encode request"))
            .expect("decode request"),
        request
    );
    assert_eq!(
        decode_response_frame(&encode_frame(&response).expect("encode response"))
            .expect("decode response"),
        response
    );
    assert_eq!(
        decode_request_frame(&encode_frame(&execute).expect("encode execute request"))
            .expect("decode execute request"),
        execute
    );
    let json = serde_json::to_string(&response).expect("response JSON");
    assert!(!json.contains("secret"));
    assert!(!json.contains("/backups/"));
}

#[test]
fn installation_deletion_plans_round_trip_as_secret_free_exact_intent() {
    let request = IpcRequest::new("delete-plan-42", IpcPayload::PlanInstallationDeletion);
    let execute = IpcRequest::new(
        "delete-execute-42",
        IpcPayload::ExecuteInstallationDeletion {
            confirmation_token: "a".repeat(64),
        },
    );
    let status_request =
        IpcRequest::new("delete-status-42", IpcPayload::InstallationDeletionStatus);
    let plan = IpcInstallationDeletionPlan::new(Vec::new(), Vec::new(), "a".repeat(64))
        .expect("empty installation deletion plan");
    let response = IpcResponse::success(
        "delete-plan-42",
        IpcResult::InstallationDeletionPlan { plan },
    );
    let status = IpcInstallationDeletionStatus::new(
        IpcInstallationLifecycle::Deleting,
        2,
        vec!["installation-delete-a".to_owned()],
        None,
        None,
    )
    .expect("deletion status");
    let status_response = IpcResponse::success(
        "delete-status-42",
        IpcResult::InstallationDeletionStatus { status },
    );

    assert_eq!(
        decode_request_frame(&encode_frame(&request).expect("encode request"))
            .expect("decode request"),
        request
    );
    assert_eq!(
        decode_response_frame(&encode_frame(&response).expect("encode response"))
            .expect("decode response"),
        response
    );
    assert_eq!(
        decode_request_frame(&encode_frame(&execute).expect("encode execute request"))
            .expect("decode execute request"),
        execute
    );
    assert_eq!(
        decode_request_frame(&encode_frame(&status_request).expect("encode status request"))
            .expect("decode status request"),
        status_request
    );
    assert_eq!(
        decode_response_frame(&encode_frame(&status_response).expect("encode status response"))
            .expect("decode status response"),
        status_response
    );
    let json = serde_json::to_string(&response).expect("response JSON");
    assert!(!json.contains("secret"));
    assert!(!json.contains("/backups/"));
}

#[test]
fn project_adoption_requests_round_trip_with_the_exact_target_path() {
    let request = IpcRequest::new(
        "adopt-42",
        IpcPayload::AdoptProject {
            canonical_path: PathBuf::from("/work/bill"),
        },
    );

    let frame = encode_frame(&request).expect("encode adoption request");
    let decoded = decode_request_frame(&frame).expect("decode adoption request");

    assert_eq!(decoded, request);
}

#[test]
fn migration_status_round_trips_without_exposing_rollback_material() {
    let request = IpcRequest::new(
        "migration-status-42",
        IpcPayload::ProjectMigrations {
            canonical_path: PathBuf::from("/work/bill"),
        },
    );
    let response = IpcResponse::success(
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
    );

    assert_eq!(
        decode_request_frame(&encode_frame(&request).expect("encode request"))
            .expect("decode request"),
        request
    );
    let decoded = decode_response_frame(&encode_frame(&response).expect("encode response"))
        .expect("decode response");
    assert_eq!(decoded, response);
    assert!(!format!("{decoded:?}").contains("recovery-point-secret"));
}

#[test]
fn image_reference_resolution_round_trips_exact_source_mappings() {
    let references = BTreeMap::from([("app".to_owned(), "ghcr.io/stackctl/php:8.4".to_owned())]);
    let request = IpcRequest::new(
        "lock-42",
        IpcPayload::ResolveImageReferences {
            references: references.clone(),
        },
    );
    let response = IpcResponse::success(
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
    );

    assert_eq!(
        decode_request_frame(&encode_frame(&request).expect("encode request"))
            .expect("decode request"),
        request
    );
    assert_eq!(
        decode_response_frame(&encode_frame(&response).expect("encode response"))
            .expect("decode response"),
        response
    );
}

#[test]
fn project_status_requests_round_trip_with_the_exact_target_path() {
    let request = IpcRequest::new(
        "status-42",
        IpcPayload::ProjectStatus {
            canonical_path: PathBuf::from("/work/bill"),
        },
    );

    let frame = encode_frame(&request).expect("encode status request");
    let decoded = decode_request_frame(&frame).expect("decode status request");

    assert_eq!(decoded, request);
}

#[test]
fn project_status_responses_preserve_typed_timestamped_health() {
    let response = IpcResponse::success(
        "status-42",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                Vec::new(),
                vec![IpcResourceStatus::new(
                    "app".to_owned(),
                    "project_application".to_owned(),
                    IpcResourceLifecycle::Active,
                    IpcResourceHealth::Unhealthy { failing_streak: 4 },
                    Some(10_000),
                    false,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
}

#[test]
fn project_status_responses_preserve_service_readiness_failures() {
    assert_eq!(
        IpcResourceHealth::ServiceNotReady { attempt: 3 }.as_str(),
        "service_not_ready"
    );
    let response = IpcResponse::success(
        "status-43",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                Vec::new(),
                vec![IpcResourceStatus::new(
                    "search".to_owned(),
                    "project_service".to_owned(),
                    IpcResourceLifecycle::Active,
                    IpcResourceHealth::ServiceNotReady { attempt: 3 },
                    Some(10_000),
                    false,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
}

#[test]
fn project_status_responses_preserve_engine_unavailability() {
    let response = IpcResponse::success(
        "status-44",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                Vec::new(),
                vec![IpcResourceStatus::new(
                    "app".to_owned(),
                    "project_application".to_owned(),
                    IpcResourceLifecycle::Active,
                    IpcResourceHealth::EngineUnavailable,
                    None,
                    false,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
    assert_eq!(
        IpcResourceHealth::EngineUnavailable.as_str(),
        "engine_unavailable"
    );
}

#[test]
fn project_status_responses_preserve_container_restart_state() {
    let response = IpcResponse::success(
        "status-45",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                Vec::new(),
                vec![IpcResourceStatus::new(
                    "app".to_owned(),
                    "project_application".to_owned(),
                    IpcResourceLifecycle::Active,
                    IpcResourceHealth::Restarting,
                    Some(10_000),
                    false,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
    assert_eq!(IpcResourceHealth::Restarting.as_str(), "restarting");
}

#[test]
fn project_status_responses_preserve_authentication_failures() {
    let response = IpcResponse::success(
        "status-46",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                Vec::new(),
                vec![IpcResourceStatus::new(
                    "search".to_owned(),
                    "project_service".to_owned(),
                    IpcResourceLifecycle::Active,
                    IpcResourceHealth::AuthenticationFailed { attempt: 2 },
                    Some(10_000),
                    false,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
    assert_eq!(
        IpcResourceHealth::AuthenticationFailed { attempt: 2 }.as_str(),
        "authentication_failed"
    );
}

#[test]
fn project_status_responses_preserve_destructive_replacement_requirements() {
    let health = IpcResourceHealth::DestructiveReplacementRequired;

    assert_eq!(health.as_str(), "destructive_replacement_required");
    let response = IpcResponse::success(
        "status-47",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                Vec::new(),
                vec![IpcResourceStatus::new(
                    "database".to_owned(),
                    "volume".to_owned(),
                    IpcResourceLifecycle::Active,
                    health,
                    Some(10_000),
                    false,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
}

#[test]
fn project_status_responses_preserve_logical_resource_drift() {
    let health = IpcResourceHealth::LogicalResourceDrift;

    assert_eq!(health.as_str(), "logical_resource_drift");
    let response = IpcResponse::success(
        "status-48",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                Vec::new(),
                vec![IpcResourceStatus::new(
                    "database".to_owned(),
                    "postgres_database".to_owned(),
                    IpcResourceLifecycle::Active,
                    health,
                    Some(10_000),
                    true,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
}

#[test]
fn project_status_responses_preserve_gateway_route_drift() {
    let health = IpcResourceHealth::GatewayRouteDrift;

    assert_eq!(health.as_str(), "gateway_route_drift");
    let response = IpcResponse::success(
        "status-49",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                vec!["bill-app.stackctl.localhost".to_owned()],
                vec![IpcResourceStatus::new(
                    "bill-app.stackctl.localhost".to_owned(),
                    "gateway_route".to_owned(),
                    IpcResourceLifecycle::Active,
                    health,
                    Some(10_000),
                    true,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
}

#[test]
fn project_status_responses_preserve_certificate_expiry() {
    let health = IpcResourceHealth::CertificateExpired;

    assert_eq!(health.as_str(), "certificate_expired");
    let response = IpcResponse::success(
        "status-50",
        IpcResult::ProjectStatus {
            project: super::IpcProjectStatus::new(
                "bill".to_owned(),
                vec!["bill-app.stackctl.localhost".to_owned()],
                vec![IpcResourceStatus::new(
                    "bill-app.stackctl.localhost".to_owned(),
                    "gateway_route".to_owned(),
                    IpcResourceLifecycle::Active,
                    health,
                    Some(10_000),
                    true,
                )],
            ),
        },
    );

    let frame = encode_frame(&response).expect("encode project status");

    assert_eq!(
        decode_response_frame(&frame).expect("decode project status"),
        response
    );
}

#[test]
fn project_environment_requests_round_trip_with_the_exact_target_path() {
    let request = IpcRequest::new(
        "environment-42",
        IpcPayload::ProjectEnvironment {
            canonical_path: PathBuf::from("/work/bill"),
        },
    );

    let frame = encode_frame(&request).expect("encode environment request");
    let decoded = decode_request_frame(&frame).expect("decode environment request");

    assert_eq!(decoded, request);
}

#[test]
fn project_log_sessions_preserve_exact_services_and_bounded_polling() {
    let open = IpcRequest::new(
        "logs-42",
        IpcPayload::OpenProjectLogs {
            canonical_path: PathBuf::from("/work/bill"),
            services: vec!["app".to_owned(), "db".to_owned()],
            follow: true,
            tail: Some(100),
        },
    );
    let poll = IpcRequest::new(
        "logs-poll-42",
        IpcPayload::PollProjectLogs {
            session_id: "logs-42".to_owned(),
            after_sequence: Some(8),
            max_chunks: 64,
        },
    );

    for request in [open, poll] {
        let frame = encode_frame(&request).expect("encode log request");
        assert_eq!(
            decode_request_frame(&frame).expect("decode log request"),
            request
        );
    }
}

#[test]
fn project_log_responses_preserve_binary_safe_ordered_chunks() {
    let response = IpcResponse::success(
        "logs-poll-42",
        IpcResult::ProjectLogs {
            session_id: "logs-42".to_owned(),
            chunks: vec![IpcLogChunk::new(
                9,
                "app".to_owned(),
                IpcOutputStream::Stderr,
                "AP8=".to_owned(),
            )],
            latest_sequence: 9,
            state: IpcLogSessionState::Streaming,
        },
    );

    let frame = encode_frame(&response).expect("encode log response");
    assert_eq!(
        decode_response_frame(&frame).expect("decode log response"),
        response
    );
}

#[test]
fn project_command_requests_preserve_typed_non_shell_arguments() {
    let request = IpcRequest::new(
        "command-42",
        IpcPayload::RunProjectCommand {
            canonical_path: PathBuf::from("/work/bill"),
            service: "app".to_owned(),
            command: IpcProjectCommand::Composer {
                arguments: vec!["install".to_owned(), "--no-interaction".to_owned()],
            },
            timeout_seconds: 300,
        },
    );

    let frame = encode_frame(&request).expect("encode command request");
    let decoded = decode_request_frame(&frame).expect("decode command request");

    assert_eq!(decoded, request);
}

#[test]
fn node_package_manager_requests_preserve_the_exact_known_executable() {
    let request = IpcRequest::new(
        "command-node-42",
        IpcPayload::RunProjectCommand {
            canonical_path: PathBuf::from("/work/bill"),
            service: "app".to_owned(),
            command: IpcProjectCommand::NodePackageManager {
                package_manager: IpcNodePackageManager::Pnpm,
                arguments: vec!["run".to_owned(), "build".to_owned()],
            },
            timeout_seconds: 300,
        },
    );

    let frame = encode_frame(&request).expect("encode command request");
    let decoded = decode_request_frame(&frame).expect("decode command request");

    assert_eq!(decoded, request);
}

#[test]
fn artisan_and_exec_requests_preserve_non_shell_arguments() {
    let commands = [
        IpcProjectCommand::Artisan {
            arguments: vec!["migrate".to_owned(), "--force".to_owned()],
            browser: false,
        },
        IpcProjectCommand::Exec {
            arguments: vec!["php".to_owned(), "-v".to_owned()],
        },
    ];

    for (index, command) in commands.into_iter().enumerate() {
        let request = IpcRequest::new(
            format!("command-structured-{index}"),
            IpcPayload::RunProjectCommand {
                canonical_path: PathBuf::from("/work/bill"),
                service: "app".to_owned(),
                command,
                timeout_seconds: 300,
            },
        );

        let frame = encode_frame(&request).expect("encode command request");
        assert_eq!(
            decode_request_frame(&frame).expect("decode command request"),
            request
        );
    }
}

#[test]
fn language_tool_requests_preserve_whitelisted_executables_and_arguments() {
    let commands = [
        IpcProjectCommand::PhpTool {
            tool: super::IpcPhpTool::Pest,
            arguments: vec!["--parallel".to_owned()],
        },
        IpcProjectCommand::Deno {
            arguments: vec!["task".to_owned(), "check".to_owned()],
        },
    ];

    for (index, command) in commands.into_iter().enumerate() {
        let request = IpcRequest::new(
            format!("command-tool-{index}"),
            IpcPayload::RunProjectCommand {
                canonical_path: PathBuf::from("/work/bill"),
                service: "app".to_owned(),
                command,
                timeout_seconds: 300,
            },
        );

        let frame = encode_frame(&request).expect("encode tool request");
        assert_eq!(
            decode_request_frame(&frame).expect("decode tool request"),
            request
        );
    }
}

#[test]
fn cancellation_targets_an_existing_request_id() {
    let request = IpcRequest::new(
        "cancel-1",
        IpcPayload::Cancel {
            target_request_id: "request-42".to_owned(),
        },
    );

    let frame = encode_frame(&request).expect("encode cancellation");
    let decoded = decode_request_frame(&frame).expect("decode cancellation");

    assert_eq!(decoded, request);
}

#[test]
fn typed_response_frames_preserve_request_correlation() {
    let response = IpcResponse::success("request-42", IpcResult::Pong);

    let frame = encode_frame(&response).expect("encode response");
    let decoded = decode_response_frame(&frame).expect("decode response");

    assert_eq!(decoded, response);
    assert_eq!(decoded.request_id(), "request-42");
}

#[test]
fn event_subscriptions_can_resume_after_a_sequence() {
    let request = IpcRequest::new(
        "events-1",
        IpcPayload::SubscribeEvents {
            after_sequence: Some(41),
        },
    );

    let frame = encode_frame(&request).expect("encode subscription");
    let decoded = decode_request_frame(&frame).expect("decode subscription");

    assert_eq!(decoded, request);
}

#[test]
fn event_journal_retention_fails_loudly_for_expired_cursors() {
    let mut journal = IpcEventJournal::new(2).expect("event journal");
    journal
        .append("operation-1", IpcEventKind::Accepted)
        .expect("accepted event");
    journal
        .append("operation-1", IpcEventKind::Completed)
        .expect("completed event");
    journal
        .append(
            "operation-2",
            IpcEventKind::Failed {
                code: "operation_failed".to_owned(),
                message: "bounded failure".to_owned(),
            },
        )
        .expect("failed event");

    let resumed = journal.events_after(Some(1)).expect("resumed events");
    assert_eq!(
        resumed
            .iter()
            .map(|event| event.sequence())
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(journal.latest_sequence(), 3);
    let error = journal
        .events_after(Some(0))
        .expect_err("expired cursor must not skip events");
    assert_eq!(
        error.to_string(),
        "event cursor 0 is no longer retained; oldest available sequence is 2"
    );

    let response = IpcResponse::success(
        "events-42",
        IpcResult::Events {
            events: resumed,
            latest_sequence: journal.latest_sequence(),
        },
    );
    let frame = encode_frame(&response).expect("encode event response");
    assert_eq!(
        decode_response_frame(&frame).expect("decode event response"),
        response
    );
}

#[test]
fn event_journal_restores_the_durable_cursor_window() {
    let journal = IpcEventJournal::restore(vec![
        DaemonEventRecord::new(
            41,
            "operation-1".to_owned(),
            r#"{"type":"accepted"}"#.to_owned(),
        ),
        DaemonEventRecord::new(
            42,
            "operation-1".to_owned(),
            r#"{"type":"completed"}"#.to_owned(),
        ),
    ])
    .expect("restore retained events");

    assert_eq!(journal.latest_sequence(), 42);
    let resumed = journal
        .events_after(Some(41))
        .expect("resume after restart");
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].sequence(), 42);
    assert_eq!(resumed[0].kind(), &IpcEventKind::Completed);
}

#[test]
fn unsupported_protocol_versions_fail_before_dispatch() {
    let frame = br#"{"protocol_version":99,"request_id":"request-1","payload":{"type":"ping"}}
"#;

    let error = decode_request_frame(frame).expect_err("unsupported protocol");

    assert_eq!(
        error.to_string(),
        "IPC protocol version 99 is unsupported; expected 1"
    );
}

#[test]
fn unknown_request_fields_are_rejected() {
    let frame = br#"{"protocol_version":1,"request_id":"request-1","surprise":true,"payload":{"type":"ping"}}
"#;

    let error = decode_request_frame(frame).expect_err("unknown field");

    assert!(error.to_string().contains("unknown field `surprise`"));
}

#[cfg(unix)]
#[test]
fn unix_listener_is_user_only_and_exclusive() {
    use super::UnixIpcListener;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let socket_path = std::env::temp_dir().join(format!(
        "stackctl-v8-ipc-{}-{unique}.sock",
        std::process::id()
    ));
    let listener = UnixIpcListener::bind(&socket_path).expect("bind IPC listener");
    let mode = std::fs::metadata(&socket_path)
        .expect("socket metadata")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(mode, 0o600);
    assert!(UnixIpcListener::bind(&socket_path).is_err());

    drop(listener);
    assert!(!socket_path.exists());
}

#[cfg(unix)]
#[test]
fn unix_listener_serves_one_bounded_correlated_request() {
    use super::UnixIpcListener;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let socket_path = std::env::temp_dir().join(format!("s8-{}-{unique}.sock", std::process::id()));
    let listener = UnixIpcListener::bind(&socket_path).expect("bind IPC listener");
    let server = std::thread::spawn(move || {
        listener
            .serve_next(|request| {
                assert_eq!(request.payload(), &IpcPayload::Ping);
                IpcResponse::success(request.request_id(), IpcResult::Pong)
            })
            .expect("serve IPC request")
    });
    let request = IpcRequest::new("ping-42", IpcPayload::Ping);
    let mut client = UnixStream::connect(&socket_path).expect("connect IPC client");
    client
        .write_all(&encode_frame(&request).expect("encode request"))
        .expect("write request");
    let mut response_frame = Vec::new();
    BufReader::new(client)
        .read_until(b'\n', &mut response_frame)
        .expect("read response");

    let response = decode_response_frame(&response_frame).expect("decode response");
    assert_eq!(response, IpcResponse::success("ping-42", IpcResult::Pong));
    assert_eq!(server.join().expect("join IPC server"), request);
}

#[cfg(unix)]
#[test]
fn unix_listener_rejects_oversized_input_before_dispatch() {
    use super::{UnixIpcListener, frame::MAX_FRAME_BYTES};
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let socket_path = std::env::temp_dir().join(format!("s8-{}-{unique}.sock", std::process::id()));
    let listener = UnixIpcListener::bind(&socket_path).expect("bind IPC listener");
    let dispatched = Arc::new(AtomicBool::new(false));
    let server_dispatched = Arc::clone(&dispatched);
    let server = std::thread::spawn(move || {
        listener.serve_next(|request| {
            server_dispatched.store(true, Ordering::Release);
            IpcResponse::success(request.request_id(), IpcResult::Pong)
        })
    });
    let mut client = UnixStream::connect(&socket_path).expect("connect IPC client");
    client
        .write_all(&vec![b'x'; MAX_FRAME_BYTES + 1])
        .expect("write oversized request");

    let error = server
        .join()
        .expect("join IPC server")
        .expect_err("oversized input must fail");
    assert!(error.to_string().contains("maximum is 1048576 bytes"));
    assert!(!dispatched.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn unix_client_sends_one_correlated_request_to_the_singleton() {
    use super::{UnixIpcListener, send_unix_request};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let socket_path = std::env::temp_dir().join(format!("s8-{}-{unique}.sock", std::process::id()));
    let listener = UnixIpcListener::bind(&socket_path).expect("bind IPC listener");
    let server = std::thread::spawn(move || {
        listener
            .serve_next(|request| IpcResponse::success(request.request_id(), IpcResult::Pong))
            .expect("serve IPC request")
    });
    let request = IpcRequest::new("client-ping", IpcPayload::Ping);

    let response = send_unix_request(&socket_path, &request, Duration::from_secs(1))
        .expect("send IPC request");

    assert_eq!(
        response,
        IpcResponse::success("client-ping", IpcResult::Pong)
    );
    assert_eq!(server.join().expect("join IPC server"), request);
}

#[cfg(unix)]
#[test]
fn unix_client_rejects_a_response_for_another_request() {
    use super::{IpcError, UnixIpcListener, send_unix_request};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let socket_path = std::env::temp_dir().join(format!("s8-{}-{unique}.sock", std::process::id()));
    let listener = UnixIpcListener::bind(&socket_path).expect("bind IPC listener");
    let server = std::thread::spawn(move || {
        listener
            .serve_next(|_| IpcResponse::success("another-request", IpcResult::Pong))
            .expect("serve IPC request")
    });
    let request = IpcRequest::new("expected-request", IpcPayload::Ping);

    let error = send_unix_request(&socket_path, &request, Duration::from_secs(1))
        .expect_err("uncorrelated response must fail");

    assert!(matches!(
        error,
        IpcError::ResponseCorrelation { expected, found }
            if expected == "expected-request" && found == "another-request"
    ));
    assert_eq!(server.join().expect("join IPC server"), request);
}
