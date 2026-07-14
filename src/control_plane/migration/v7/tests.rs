use super::{
    V7EnvironmentMigrationAdapter, V7GeneratedEnvironmentRollbackOptions,
    V7HostArtifactDiscoveryOptions, V7InventoryBlocker, V7MigrationAdapterExecutor,
    V7MigrationAdapterRegistry, V7MigrationAdapterSelectionOptions, V7MigrationAdapterTarget,
    V7MigrationCutoverOptions, V7MigrationExecutionJournal, V7MigrationExecutionPlanOptions,
    V7MigrationRollbackOptions, V7MigrationRouteSource, V7MigrationServiceAdapter,
    V7MigrationServiceSource, V7ProjectInventory, V7ProjectInventoryOptions,
    V7ProjectInventoryRequest, V7ProtectedGeneratedEnvironmentAdapterOptions,
    V7RouteMigrationAdapter, V7RuntimeFeature, V7TrustMigrationAdapter, V7VolumeMigrationAdapter,
    V7VolumeSource, capture_v7_generated_environment_rollback, confirm_v7_migration,
    cutover_v7_migration, inventory_v7_host_artifacts, inventory_v7_project,
    plan_v7_migration_execution, prepare_v7_migration, read_v7_generated_environment_rollback,
    register_v7_no_op_migration_adapters, register_v7_protected_environment_migration_adapter,
    rollback_v7_migration, select_v7_migration_adapters,
};
use crate::config::{
    Config, Driver, HookOnError, HookPhase, HookRun, Kind, ProjectType, ServiceConfig, ServiceHook,
};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::engine::{
    ContainerId, EngineFuture, LegacyContainerDiscovery, ObservedContainer, ObservedContainerMount,
};
use crate::control_plane::migration::{
    MigrationBackup, MigrationCutoverPlan, MigrationFuture, MigrationOperationError,
    MigrationRollbackPlan,
};
use crate::control_plane::state::{
    AcceptedV7EnvironmentRollback, AcceptedV7InventoryRecord, AcceptedV7InventoryRecordOptions,
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions, ProjectRecord,
    StateStoreError, V7MigrationAdapterCheckpoint, V7MigrationExecutionPhase,
    V7MigrationExecutionRecord, V7MigrationExecutionRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[test]
fn v7_adapter_selection_is_complete_deterministic_and_does_not_archive_logical_storage() {
    let services = vec![
        migration_source("app", "frankenphp", &["bill-app-data"]),
        migration_source("database", "postgres", &["bill-database-data"]),
        migration_source("cache", "redis", &["bill-cache-data"]),
        migration_source("cloud", "localstack", &["bill-cloud-data"]),
        migration_source("browser", "dusk", &[]),
    ];
    let routes = vec![V7MigrationRouteSource {
        service_id: "app",
        domain: "bill-app.stackctl.localhost",
        scheme: "https",
        host_port: 8443,
    }];
    let options = V7MigrationAdapterSelectionOptions {
        evidence_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        services: &services,
        routes: &routes,
        requires_legacy_ca_capture: true,
        captured_ca_certificates: 1,
        generated_environment_present: true,
        protected_generated_environment: true,
    };

    let first = select_v7_migration_adapters(options).expect("adapter plan");
    let mut reversed_services = services.clone();
    reversed_services.reverse();
    let second = select_v7_migration_adapters(V7MigrationAdapterSelectionOptions {
        services: &reversed_services,
        ..options
    })
    .expect("stable adapter plan");

    assert_eq!(first, second);
    assert_eq!(first.evidence_revision(), options.evidence_revision);
    assert_eq!(first.plan_revision().len(), 64);
    assert_eq!(
        first
            .services()
            .iter()
            .map(|selection| (selection.service_id(), selection.adapter()))
            .collect::<Vec<_>>(),
        vec![
            ("app", &V7MigrationServiceAdapter::RecreateProjectWorkload),
            ("browser", &V7MigrationServiceAdapter::RecreateEphemeral),
            ("cache", &V7MigrationServiceAdapter::RedisTenantPrefix),
            ("cloud", &V7MigrationServiceAdapter::RecreateStateless),
            (
                "database",
                &V7MigrationServiceAdapter::PostgresLogicalDatabase,
            ),
        ]
    );
    assert_eq!(first.services()[0].named_volumes(), ["bill-app-data"]);
    assert_eq!(
        first.services()[0].volume_adapter(),
        V7VolumeMigrationAdapter::NamedVolumeArchive
    );
    assert_eq!(
        first.services()[0].deployment_strategy(),
        ServiceDeploymentStrategy::ProjectApplication
    );
    assert_eq!(
        first.services()[2].deployment_strategy(),
        ServiceDeploymentStrategy::SharedByCompatibility
    );
    assert_eq!(
        first.services()[3].deployment_strategy(),
        ServiceDeploymentStrategy::DedicatedProject
    );
    assert_eq!(
        first.services()[3].volume_adapter(),
        V7VolumeMigrationAdapter::NamedVolumeArchive
    );
    assert!(first.services()[4].named_volumes().is_empty());
    assert_eq!(
        first.services()[4].volume_adapter(),
        V7VolumeMigrationAdapter::LogicalDataOwnsStorage
    );
    assert_eq!(
        first.route_adapter(),
        V7RouteMigrationAdapter::GatewaySnapshotCutover
    );
    assert_eq!(
        first.trust_adapter(),
        V7TrustMigrationAdapter::InstallationLegacyCaddyCaTransition
    );
    assert_eq!(
        first.environment_adapter(),
        V7EnvironmentMigrationAdapter::ProtectedGeneratedEnvironment
    );

    let execution = plan_v7_migration_execution(V7MigrationExecutionPlanOptions {
        project_id: "bill",
        canonical_project_path: Path::new("/work/bill"),
        adapter_plan: &first,
        planned_at_unix_seconds: 20,
    })
    .expect("durable adapter execution plan");
    assert_eq!(execution.phase(), V7MigrationExecutionPhase::Planned);
    assert_eq!(execution.checkpoints().len(), 13);
    assert_eq!(execution.evidence_revision(), first.evidence_revision());
    assert_eq!(execution.adapter_plan_revision(), first.plan_revision());
    for (adapter_id, requires_recovery) in [
        ("environment", true),
        ("route", true),
        ("service/app", false),
        ("service/database", true),
        ("trust", true),
        ("volume/app", true),
        ("volume/database", false),
    ] {
        let checkpoint = execution
            .checkpoints()
            .iter()
            .find(|checkpoint| checkpoint.adapter_id() == adapter_id)
            .unwrap_or_else(|| panic!("checkpoint '{adapter_id}'"));
        assert_eq!(
            checkpoint.requires_recovery(),
            requires_recovery,
            "checkpoint {adapter_id}"
        );
    }
}

#[test]
fn v7_preparation_resumes_after_failure_without_repeating_verified_recovery() {
    run_test(async {
        let planned = V7MigrationExecutionRecord::new(V7MigrationExecutionRecordOptions {
            project_id: "bill".to_owned(),
            canonical_project_path: "/work/bill".into(),
            evidence_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_owned(),
            adapter_plan_revision:
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
            phase: V7MigrationExecutionPhase::Planned,
            checkpoints: vec![
                V7MigrationAdapterCheckpoint::pending(
                    "service/database",
                    "postgres-logical-database",
                    true,
                    10,
                )
                .expect("database checkpoint"),
                V7MigrationAdapterCheckpoint::pending("route", "no-routes", false, 10)
                    .expect("route checkpoint"),
            ],
            updated_at_unix_seconds: 10,
        })
        .expect("planned execution");
        let calls = Arc::new(Mutex::new(Vec::new()));
        let fail_database_target = Arc::new(AtomicBool::new(true));
        let incomplete_executor: Box<dyn V7MigrationAdapterExecutor> =
            Box::new(RecordingV7Adapter {
                calls: Arc::clone(&calls),
                target_reference: None,
                fail_target_once: Arc::new(AtomicBool::new(false)),
                fail_cutover_once: Arc::new(AtomicBool::new(false)),
            });
        let mut incomplete_registry = V7MigrationAdapterRegistry::default();
        incomplete_registry
            .register("route", "no-routes", incomplete_executor)
            .expect("register incomplete route adapter");
        let mut incomplete_journal = RecordingV7Journal::default();
        assert_eq!(
            prepare_v7_migration(
                &mut incomplete_journal,
                &planned,
                &mut incomplete_registry,
                20,
            )
            .await
            .expect_err("incomplete registry fails before persistence")
            .to_string(),
            "v7 migration adapter registry has 1 entries, expected 2"
        );
        assert!(incomplete_journal.writes.is_empty());
        assert!(calls.lock().expect("preflight calls").is_empty());

        let database_executor: Box<dyn V7MigrationAdapterExecutor> = Box::new(RecordingV7Adapter {
            calls: Arc::clone(&calls),
            target_reference: Some("postgres-v8-bill"),
            fail_target_once: Arc::clone(&fail_database_target),
            fail_cutover_once: Arc::new(AtomicBool::new(false)),
        });
        let route_executor: Box<dyn V7MigrationAdapterExecutor> = Box::new(RecordingV7Adapter {
            calls: Arc::clone(&calls),
            target_reference: None,
            fail_target_once: Arc::new(AtomicBool::new(false)),
            fail_cutover_once: Arc::new(AtomicBool::new(false)),
        });
        let mut registry = V7MigrationAdapterRegistry::default();
        registry
            .register(
                "service/database",
                "postgres-logical-database",
                database_executor,
            )
            .expect("register database adapter");
        registry
            .register("route", "no-routes", route_executor)
            .expect("register route adapter");
        let mut journal = RecordingV7Journal::default();

        let error = prepare_v7_migration(&mut journal, &planned, &mut registry, 20)
            .await
            .expect_err("first target preparation fails");
        assert_eq!(
            error.to_string(),
            "v7 migration adapter 'service/database' target preparation failed: target unavailable"
        );
        assert_eq!(
            journal
                .current
                .as_ref()
                .map(V7MigrationExecutionRecord::phase),
            Some(V7MigrationExecutionPhase::Preparing)
        );
        assert_eq!(
            journal
                .current
                .as_ref()
                .expect("recovery checkpoint")
                .checkpoints()[1]
                .recovery_reference(),
            Some("backup:service/database")
        );

        let prepared = prepare_v7_migration(&mut journal, &planned, &mut registry, 21)
            .await
            .expect("resume preparation");
        assert_eq!(prepared.phase(), V7MigrationExecutionPhase::Prepared);
        assert_eq!(
            calls.lock().expect("calls").as_slice(),
            [
                "recovery:service/database",
                "target:service/database",
                "target:service/database",
                "target:route",
            ]
        );
        assert_eq!(
            journal
                .writes
                .iter()
                .map(V7MigrationExecutionRecord::phase)
                .collect::<Vec<_>>(),
            [
                V7MigrationExecutionPhase::Planned,
                V7MigrationExecutionPhase::Preparing,
                V7MigrationExecutionPhase::Preparing,
                V7MigrationExecutionPhase::Preparing,
                V7MigrationExecutionPhase::Prepared,
            ]
        );

        let replay = prepare_v7_migration(&mut journal, &planned, &mut registry, 22)
            .await
            .expect("prepared replay");
        assert_eq!(replay, prepared);
        assert_eq!(calls.lock().expect("replay calls").len(), 4);
    });
}

#[test]
fn v7_cutover_replays_as_one_barrier_and_supports_confirmation_or_rollback() {
    run_test(async {
        let database = V7MigrationAdapterCheckpoint::pending(
            "service/database",
            "postgres-logical-database",
            true,
            10,
        )
        .expect("database checkpoint");
        let route = V7MigrationAdapterCheckpoint::pending("route", "no-routes", false, 10)
            .expect("route checkpoint");
        let plan = v7_execution_record(
            V7MigrationExecutionPhase::Planned,
            vec![database.clone(), route.clone()],
            10,
        );
        let database = database
            .with_recovery_verified(
                "backup:database",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                100,
                11,
            )
            .expect("database recovery")
            .with_target_verified(Some("postgres-v8-bill"), 12)
            .expect("database target");
        let route = route.with_target_verified(None, 12).expect("route target");
        let prepared = v7_execution_record(
            V7MigrationExecutionPhase::Prepared,
            vec![database, route],
            12,
        );
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = lifecycle_v7_registry(Arc::clone(&calls), true);
        let mut journal = RecordingV7Journal {
            current: Some(prepared.clone()),
            writes: Vec::new(),
        };

        let desired_state = v7_cutover_state();
        let error = cutover_v7_migration(V7MigrationCutoverOptions {
            journal: &mut journal,
            plan: &plan,
            registry: &mut registry,
            desired_state: &desired_state,
            updated_at_unix_seconds: 20,
        })
        .await
        .expect_err("route cutover fails once");
        assert_eq!(
            error.to_string(),
            "v7 migration adapter 'route' cutover failed: cutover unavailable"
        );
        assert_eq!(journal.current, Some(prepared));
        assert_eq!(
            calls.lock().expect("failed cutover calls").as_slice(),
            ["cutover:service/database", "cutover:route"]
        );

        calls.lock().expect("clear calls").clear();
        let cutover = cutover_v7_migration(V7MigrationCutoverOptions {
            journal: &mut journal,
            plan: &plan,
            registry: &mut registry,
            desired_state: &desired_state,
            updated_at_unix_seconds: 21,
        })
        .await
        .expect("replayed cutover");
        assert_eq!(cutover.phase(), V7MigrationExecutionPhase::Cutover);
        assert_eq!(
            calls.lock().expect("replayed cutover calls").as_slice(),
            ["cutover:service/database", "cutover:route"]
        );

        let mut confirmation_journal = journal.clone();
        calls.lock().expect("clear calls").clear();
        let restored_state = v7_rollback_state();
        let rolled_back = rollback_v7_migration(V7MigrationRollbackOptions {
            journal: &mut journal,
            plan: &plan,
            registry: &mut registry,
            restored_state: &restored_state,
            updated_at_unix_seconds: 22,
        })
        .await
        .expect("rollback cutover");
        assert_eq!(rolled_back.phase(), V7MigrationExecutionPhase::RolledBack);
        assert_eq!(
            calls.lock().expect("rollback calls").as_slice(),
            ["rollback:route", "rollback:service/database"]
        );

        calls.lock().expect("clear calls").clear();
        let confirmed = confirm_v7_migration(&mut confirmation_journal, &plan, &mut registry, 22)
            .await
            .expect("confirm cutover");
        assert_eq!(confirmed.phase(), V7MigrationExecutionPhase::Confirmed);
        assert_eq!(
            calls.lock().expect("confirmation calls").as_slice(),
            ["confirm:route", "confirm:service/database"]
        );
    });
}

#[test]
fn v7_cutover_rejects_mismatched_desired_state_before_side_effects() {
    run_test(async {
        let route = V7MigrationAdapterCheckpoint::pending("route", "no-routes", false, 10)
            .expect("route checkpoint");
        let plan = v7_execution_record(V7MigrationExecutionPhase::Planned, vec![route.clone()], 10);
        let prepared = v7_execution_record(
            V7MigrationExecutionPhase::Prepared,
            vec![route.with_target_verified(None, 11).expect("route target")],
            11,
        );
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = V7MigrationAdapterRegistry::default();
        registry
            .register(
                "route",
                "no-routes",
                Box::new(RecordingV7Adapter {
                    calls: Arc::clone(&calls),
                    target_reference: None,
                    fail_target_once: Arc::new(AtomicBool::new(false)),
                    fail_cutover_once: Arc::new(AtomicBool::new(false)),
                }),
            )
            .expect("register route adapter");
        let mut journal = RecordingV7Journal {
            current: Some(prepared.clone()),
            writes: Vec::new(),
        };
        let desired_state = v7_cutover_state_at("/work/other");

        let error = cutover_v7_migration(V7MigrationCutoverOptions {
            journal: &mut journal,
            plan: &plan,
            registry: &mut registry,
            desired_state: &desired_state,
            updated_at_unix_seconds: 12,
        })
        .await
        .expect_err("reject mismatched desired state");

        assert_eq!(
            error.to_string(),
            "invalid v7 execution plan: desired project state does not match the immutable execution identity"
        );
        assert!(calls.lock().expect("side effects").is_empty());
        assert_eq!(journal.current, Some(prepared));
    });
}

#[test]
fn v7_rollback_rejects_mismatched_restored_state_before_side_effects() {
    run_test(async {
        let route = V7MigrationAdapterCheckpoint::pending("route", "no-routes", false, 10)
            .expect("route checkpoint");
        let plan = v7_execution_record(V7MigrationExecutionPhase::Planned, vec![route.clone()], 10);
        let cutover = v7_execution_record(
            V7MigrationExecutionPhase::Cutover,
            vec![
                route
                    .with_target_verified(None, 11)
                    .expect("route target")
                    .with_cutover(12)
                    .expect("route cutover"),
            ],
            12,
        );
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = V7MigrationAdapterRegistry::default();
        registry
            .register(
                "route",
                "no-routes",
                Box::new(RecordingV7Adapter {
                    calls: Arc::clone(&calls),
                    target_reference: None,
                    fail_target_once: Arc::new(AtomicBool::new(false)),
                    fail_cutover_once: Arc::new(AtomicBool::new(false)),
                }),
            )
            .expect("register route adapter");
        let mut journal = RecordingV7Journal {
            current: Some(cutover.clone()),
            writes: Vec::new(),
        };
        let restored_state = MigrationRollbackPlan::new(
            ProjectRecord::new(
                "/work/other".into(),
                "bill".to_owned(),
                vec!["bill-legacy.stackctl.localhost".to_owned()],
            ),
            ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
                project_id: "bill".to_owned(),
                revision: "sha256:v7-environment".to_owned(),
                values: BTreeMap::new(),
                lifecycle: EnvironmentLifecycle::Active,
            }),
            Vec::new(),
        )
        .expect("mismatched rollback state");

        let error = rollback_v7_migration(V7MigrationRollbackOptions {
            journal: &mut journal,
            plan: &plan,
            registry: &mut registry,
            restored_state: &restored_state,
            updated_at_unix_seconds: 13,
        })
        .await
        .expect_err("reject mismatched restored state");

        assert_eq!(
            error.to_string(),
            "invalid v7 execution plan: restored project state does not match the immutable execution identity"
        );
        assert!(calls.lock().expect("side effects").is_empty());
        assert_eq!(journal.current, Some(cutover));
    });
}

#[test]
fn explicit_v7_no_op_strategies_participate_in_the_complete_lifecycle() {
    run_test(async {
        let checkpoints = [
            ("environment", "no-generated-environment"),
            ("route", "no-routes"),
            ("trust", "no-legacy-trust-transition"),
            ("volume/cache", "no-named-volumes"),
            ("volume/database", "logical-data-owns-storage"),
        ]
        .into_iter()
        .map(|(adapter_id, adapter_kind)| {
            V7MigrationAdapterCheckpoint::pending(adapter_id, adapter_kind, false, 10)
                .expect("no-op checkpoint")
        })
        .collect();
        let plan = v7_execution_record(V7MigrationExecutionPhase::Planned, checkpoints, 10);
        let mut registry = V7MigrationAdapterRegistry::default();
        assert_eq!(
            register_v7_no_op_migration_adapters(&mut registry, &plan)
                .expect("register no-op strategies"),
            5
        );
        let mut journal = RecordingV7Journal::default();

        let prepared = prepare_v7_migration(&mut journal, &plan, &mut registry, 11)
            .await
            .expect("prepare no-op strategies");
        assert_eq!(prepared.phase(), V7MigrationExecutionPhase::Prepared);
        assert!(
            prepared
                .checkpoints()
                .iter()
                .all(|checkpoint| checkpoint.target_reference().is_none())
        );
        let desired_state = v7_cutover_state();
        let cutover = cutover_v7_migration(V7MigrationCutoverOptions {
            journal: &mut journal,
            plan: &plan,
            registry: &mut registry,
            desired_state: &desired_state,
            updated_at_unix_seconds: 12,
        })
        .await
        .expect("cut over no-op strategies");
        assert_eq!(cutover.phase(), V7MigrationExecutionPhase::Cutover);
        let confirmed = confirm_v7_migration(&mut journal, &plan, &mut registry, 13)
            .await
            .expect("confirm no-op strategies");
        assert_eq!(confirmed.phase(), V7MigrationExecutionPhase::Confirmed);
    });
}

#[test]
fn v7_adapter_selection_covers_every_legacy_driver_without_fallback() {
    use V7MigrationServiceAdapter as Adapter;

    let cases = [
        ("mongodb", Adapter::MongoDbLogicalDatabase),
        ("memcached", Adapter::RecreateStateless),
        ("postgres", Adapter::PostgresLogicalDatabase),
        ("mysql", Adapter::MySqlLogicalDatabase),
        ("sqlserver", Adapter::SqlServerLogicalDatabase),
        ("redis", Adapter::RedisTenantPrefix),
        ("valkey", Adapter::ValkeyTenantPrefix),
        ("dragonfly", Adapter::RecreateStateless),
        ("minio", Adapter::MinioBucket),
        ("garage", Adapter::RecreateStateless),
        ("rustfs", Adapter::RecreateStateless),
        ("localstack", Adapter::RecreateStateless),
        ("opensearch", Adapter::RecreateStateless),
        ("elasticsearch", Adapter::RecreateStateless),
        ("meilisearch", Adapter::RecreateStateless),
        ("typesense", Adapter::RecreateStateless),
        ("frankenphp", Adapter::RecreateProjectWorkload),
        ("reverb", Adapter::RecreateProjectWorkload),
        ("horizon", Adapter::RecreateProjectWorkload),
        ("scheduler", Adapter::RecreateProjectWorkload),
        ("dusk", Adapter::RecreateEphemeral),
        ("gotenberg", Adapter::RecreateStateless),
        ("mailhog", Adapter::RecreateStateless),
        ("rabbitmq", Adapter::RabbitMqVhost),
        ("soketi", Adapter::RecreateStateless),
    ];

    for (driver, expected) in cases {
        let services = vec![migration_source("service", driver, &[])];
        let plan = select_v7_migration_adapters(V7MigrationAdapterSelectionOptions {
            evidence_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            services: &services,
            routes: &[],
            requires_legacy_ca_capture: false,
            captured_ca_certificates: 0,
            generated_environment_present: false,
            protected_generated_environment: false,
        })
        .unwrap_or_else(|error| panic!("driver '{driver}' must select exactly: {error}"));

        assert_eq!(plan.services()[0].adapter(), &expected, "driver {driver}");
    }
}

#[test]
fn v7_adapter_selection_fails_closed_for_unknown_drivers_and_route_drift() {
    let unknown = vec![migration_source("database", "invented", &[])];
    let error = select_v7_migration_adapters(V7MigrationAdapterSelectionOptions {
        evidence_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        services: &unknown,
        routes: &[],
        requires_legacy_ca_capture: false,
        captured_ca_certificates: 0,
        generated_environment_present: false,
        protected_generated_environment: false,
    })
    .expect_err("unknown driver must block");
    assert_eq!(
        error.to_string(),
        "v7 service 'database' driver 'invented' has no migration adapter"
    );

    let services = vec![migration_source("app", "frankenphp", &[])];
    let routes = vec![V7MigrationRouteSource {
        service_id: "missing",
        domain: "bill-app.stackctl.localhost",
        scheme: "https",
        host_port: 8443,
    }];
    let error = select_v7_migration_adapters(V7MigrationAdapterSelectionOptions {
        evidence_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        services: &services,
        routes: &routes,
        requires_legacy_ca_capture: false,
        captured_ca_certificates: 0,
        generated_environment_present: false,
        protected_generated_environment: false,
    })
    .expect_err("route drift must block");
    assert_eq!(
        error.to_string(),
        "v7 route 'bill-app.stackctl.localhost' references unknown service 'missing'"
    );

    let services = vec![migration_source("database", "postgres", &[])];
    let routes = vec![V7MigrationRouteSource {
        service_id: "database",
        domain: "bill-database.stackctl.localhost",
        scheme: "https",
        host_port: 5432,
    }];
    let error = select_v7_migration_adapters(V7MigrationAdapterSelectionOptions {
        evidence_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        services: &services,
        routes: &routes,
        requires_legacy_ca_capture: false,
        captured_ca_certificates: 0,
        generated_environment_present: false,
        protected_generated_environment: false,
    })
    .expect_err("unroutable target must block");
    assert_eq!(
        error.to_string(),
        "v7 route 'bill-database.stackctl.localhost' service 'database' cannot claim a v8 gateway route with deployment strategy 'shared-by-compatibility'"
    );
}

#[test]
fn v7_adapter_selection_requires_protected_environment_and_captured_trust() {
    let services = vec![migration_source("app", "frankenphp", &[])];
    let base = V7MigrationAdapterSelectionOptions {
        evidence_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        services: &services,
        routes: &[],
        requires_legacy_ca_capture: true,
        captured_ca_certificates: 0,
        generated_environment_present: false,
        protected_generated_environment: false,
    };
    assert_eq!(
        select_v7_migration_adapters(base)
            .expect_err("missing CA must block")
            .to_string(),
        "v7 migration requires legacy Caddy CA capture but accepted evidence contains no certificate"
    );

    let error = select_v7_migration_adapters(V7MigrationAdapterSelectionOptions {
        requires_legacy_ca_capture: false,
        generated_environment_present: true,
        ..base
    })
    .expect_err("unprotected environment must block");
    assert_eq!(
        error.to_string(),
        "v7 migration requires protected generated-environment rollback before adapter selection"
    );
}

fn migration_source<'source>(
    service_id: &'source str,
    driver: &'source str,
    named_volumes: &'source [&'source str],
) -> V7MigrationServiceSource<'source> {
    V7MigrationServiceSource {
        service_id,
        driver,
        named_volumes,
    }
}

#[test]
fn v7_host_artifacts_capture_routes_trust_and_secret_free_environment_metadata() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v7-host-artifacts-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("artifact fixture");
    let environment_path = root.join(".env");
    let hosts_path = root.join("hosts");
    let caddy_state_path = root.join("sites.toml");
    let caddy_ca_path = root.join("root.crt");
    std::fs::write(
        &environment_path,
        "DB_PASSWORD=database-secret\nAPP_URL=https://bill.test\n# ignored\n",
    )
    .expect("legacy environment");
    std::fs::write(
        &hosts_path,
        "127.0.0.1 localhost bill.test\n127.0.0.1 other.test\n",
    )
    .expect("legacy hosts");
    std::fs::write(
        &caddy_state_path,
        "[routes]\n\"bill.test\" = \"127.0.0.1:8080\"\n\"other.test\" = \"127.0.0.1:9090\"\n",
    )
    .expect("legacy Caddy state");
    std::fs::write(&caddy_ca_path, "public-certificate").expect("legacy Caddy CA");
    let domains = vec!["bill.test".to_owned(), "missing.test".to_owned()];

    let artifacts = inventory_v7_host_artifacts(V7HostArtifactDiscoveryOptions {
        environment_path: &environment_path,
        hosts_path: &hosts_path,
        caddy_state_path: &caddy_state_path,
        caddy_ca_candidates: std::slice::from_ref(&caddy_ca_path),
        route_domains: &domains,
        maximum_artifact_bytes: 1024 * 1024,
    })
    .expect("legacy host artifacts");

    let environment = artifacts
        .generated_environment()
        .expect("generated environment metadata");
    assert_eq!(environment.path(), environment_path);
    assert_eq!(environment.keys(), ["APP_URL", "DB_PASSWORD"]);
    assert!(environment.size_bytes() > 0);
    assert_eq!(artifacts.hosts_domains(), ["bill.test"]);
    assert_eq!(
        artifacts
            .caddy_routes()
            .get("bill.test")
            .map(String::as_str),
        Some("127.0.0.1:8080")
    );
    assert!(!artifacts.caddy_routes().contains_key("other.test"));
    assert_eq!(artifacts.caddy_ca_certificates().len(), 1);
    assert_eq!(
        artifacts.caddy_ca_certificates()[0].revision().len(),
        "sha256:".len() + 64
    );
    let debug = format!("{artifacts:?}");
    assert!(!debug.contains("database-secret"));

    std::fs::remove_dir_all(root).expect("remove artifact fixture");
}

#[cfg(unix)]
#[test]
fn v7_host_artifacts_reject_symlinks_and_files_over_the_bound() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "stackctl-v7-host-artifact-guard-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("artifact fixture");
    let environment_path = root.join(".env");
    let environment_target = root.join("actual.env");
    let hosts_path = root.join("hosts");
    let caddy_state_path = root.join("missing-sites.toml");
    std::fs::write(&environment_target, "SECRET=secret\n").expect("environment target");
    symlink(&environment_target, &environment_path).expect("environment symlink");
    std::fs::write(&hosts_path, "127.0.0.1 localhost\n").expect("legacy hosts");

    let error = inventory_v7_host_artifacts(V7HostArtifactDiscoveryOptions {
        environment_path: &environment_path,
        hosts_path: &hosts_path,
        caddy_state_path: &caddy_state_path,
        caddy_ca_candidates: &[],
        route_domains: &[],
        maximum_artifact_bytes: 1024,
    })
    .expect_err("symlinked legacy environment");

    assert!(
        error
            .to_string()
            .contains("must be a regular non-symlink file")
    );

    std::fs::remove_file(&environment_path).expect("remove environment symlink");
    std::fs::write(&environment_path, "SECRET=secret\n").expect("legacy environment");
    let error = inventory_v7_host_artifacts(V7HostArtifactDiscoveryOptions {
        environment_path: &environment_path,
        hosts_path: &hosts_path,
        caddy_state_path: &caddy_state_path,
        caddy_ca_candidates: &[],
        route_domains: &[],
        maximum_artifact_bytes: 5,
    })
    .expect_err("oversize legacy environment");

    assert!(error.to_string().contains("exceeds the 5 byte limit"));

    std::fs::remove_dir_all(root).expect("remove artifact fixture");
}

#[cfg(unix)]
#[test]
fn v7_generated_environment_rollback_is_private_randomized_and_exact() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "stackctl-v7-environment-rollback-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let project = root.join("bill");
    let backup_root = root.join("backups");
    std::fs::create_dir_all(&project).expect("project fixture");
    let environment_path = project.join(".env");
    let hosts_path = root.join("hosts");
    let caddy_state_path = root.join("missing-sites.toml");
    let environment_bytes = b"DB_PASSWORD=small-secret\nEMPTY=\n";
    std::fs::write(&environment_path, environment_bytes).expect("legacy environment");
    std::fs::write(&hosts_path, "127.0.0.1 localhost\n").expect("legacy hosts");
    let artifacts = inventory_v7_host_artifacts(V7HostArtifactDiscoveryOptions {
        environment_path: &environment_path,
        hosts_path: &hosts_path,
        caddy_state_path: &caddy_state_path,
        caddy_ca_candidates: &[],
        route_domains: &[],
        maximum_artifact_bytes: 1024,
    })
    .expect("legacy host inventory");
    let expected = artifacts
        .generated_environment()
        .expect("generated environment evidence");
    let evidence_revision = "a".repeat(64);

    let rollback =
        capture_v7_generated_environment_rollback(V7GeneratedEnvironmentRollbackOptions {
            project_id: "bill",
            evidence_revision: &evidence_revision,
            expected,
            backup_root: &backup_root,
            maximum_environment_bytes: 1024,
            created_at_unix_seconds: 40_000,
        })
        .expect("protected environment rollback");

    assert!(rollback.recovery_point().is_absolute());
    assert_eq!(rollback.artifact_sha256().len(), 64);
    assert!(rollback.artifact_size_bytes() > environment_bytes.len() as u64);
    let stored_bytes = std::fs::read(rollback.recovery_point().join("artifact.bin"))
        .expect("stored rollback envelope");
    assert!(
        !stored_bytes
            .windows(b"small-secret".len())
            .any(|window| window == b"small-secret")
    );
    assert_eq!(
        std::fs::metadata(rollback.recovery_point())
            .expect("rollback directory metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(rollback.recovery_point().join("artifact.bin"))
            .expect("rollback artifact metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let restored =
        read_v7_generated_environment_rollback(&rollback, "bill", &evidence_revision, 40_001, 1024)
            .expect("read protected environment rollback");
    assert_eq!(restored, environment_bytes);

    std::fs::write(&environment_path, b"DB_PASSWORD=changed-after-inventory\n")
        .expect("change legacy environment");
    let error = capture_v7_generated_environment_rollback(V7GeneratedEnvironmentRollbackOptions {
        project_id: "bill",
        evidence_revision: &evidence_revision,
        expected,
        backup_root: &backup_root,
        maximum_environment_bytes: 1024,
        created_at_unix_seconds: 40_002,
    })
    .expect_err("changed environment must require a new plan");
    assert!(error.to_string().contains("changed after inventory"));

    std::fs::remove_dir_all(root).expect("remove rollback fixture");
}

#[cfg(unix)]
#[test]
fn protected_v7_environment_adapter_reuses_recovery_without_mutating_dotenv() {
    run_test(async {
        let root = std::env::temp_dir().join(format!(
            "stackctl-v7-environment-adapter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let project_path = root.join("bill");
        let backup_root = root.join("backups");
        let environment_path = project_path.join(".env");
        let hosts_path = root.join("hosts");
        let environment_bytes = b"DB_PASSWORD=user-owned\n";
        std::fs::create_dir_all(&project_path).expect("project fixture");
        std::fs::write(&environment_path, environment_bytes).expect("legacy environment");
        std::fs::write(&hosts_path, "127.0.0.1 localhost\n").expect("legacy hosts");
        let artifacts = inventory_v7_host_artifacts(V7HostArtifactDiscoveryOptions {
            environment_path: &environment_path,
            hosts_path: &hosts_path,
            caddy_state_path: &root.join("missing-caddy.toml"),
            caddy_ca_candidates: &[],
            route_domains: &[],
            maximum_artifact_bytes: 1024,
        })
        .expect("legacy host inventory");
        let source_revision = format!("sha256:{}", "a".repeat(64));
        let inventory_json = serde_json::json!({
            "project_id": "bill",
            "canonical_project_path": project_path,
            "source_revision": source_revision,
            "blockers": [],
            "host_artifacts": { "generated_environment": { "path": environment_path } }
        })
        .to_string();
        let evidence_revision = hex::encode(Sha256::digest(inventory_json.as_bytes()));
        let rollback =
            capture_v7_generated_environment_rollback(V7GeneratedEnvironmentRollbackOptions {
                project_id: "bill",
                evidence_revision: &evidence_revision,
                expected: artifacts
                    .generated_environment()
                    .expect("generated environment evidence"),
                backup_root: &backup_root,
                maximum_environment_bytes: 1024,
                created_at_unix_seconds: 40_000,
            })
            .expect("protected environment rollback");
        let accepted = AcceptedV7InventoryRecord::new(AcceptedV7InventoryRecordOptions {
            project_id: "bill".to_owned(),
            canonical_project_path: project_path.clone(),
            source_revision,
            inventory_json,
            generated_environment_rollback: Some(
                AcceptedV7EnvironmentRollback::new(
                    rollback.recovery_point().to_path_buf(),
                    rollback.artifact_sha256().to_owned(),
                    rollback.artifact_size_bytes(),
                )
                .expect("accepted rollback"),
            ),
            accepted_at_unix_seconds: 40_001,
        })
        .expect("accepted inventory");
        let target_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
            project_id: "bill".to_owned(),
            revision: "sha256:v8-environment".to_owned(),
            values: BTreeMap::from([("DB_PASSWORD".to_owned(), "managed".to_owned())]),
            lifecycle: EnvironmentLifecycle::Active,
        });
        let checkpoint = V7MigrationAdapterCheckpoint::pending(
            "environment",
            "protected-generated-environment",
            true,
            40_001,
        )
        .expect("environment checkpoint");
        let plan = V7MigrationExecutionRecord::new(V7MigrationExecutionRecordOptions {
            project_id: "bill".to_owned(),
            canonical_project_path: project_path.clone(),
            evidence_revision: accepted.evidence_revision().to_owned(),
            adapter_plan_revision: "b".repeat(64),
            phase: V7MigrationExecutionPhase::Planned,
            checkpoints: vec![checkpoint],
            updated_at_unix_seconds: 40_001,
        })
        .expect("environment execution");
        let mut registry = V7MigrationAdapterRegistry::default();
        assert!(
            register_v7_protected_environment_migration_adapter(
                &mut registry,
                &plan,
                V7ProtectedGeneratedEnvironmentAdapterOptions {
                    accepted: &accepted,
                    target_environment: &target_environment,
                    verified_at_unix_seconds: 40_002,
                    maximum_environment_bytes: 1024,
                },
            )
            .expect("register protected environment adapter")
        );
        let mut journal = RecordingV7Journal::default();
        let prepared = prepare_v7_migration(&mut journal, &plan, &mut registry, 40_002)
            .await
            .expect("prepare protected environment");
        assert_eq!(
            prepared.checkpoints()[0].recovery_reference(),
            rollback.recovery_point().to_str()
        );
        assert_eq!(
            prepared.checkpoints()[0].target_reference(),
            Some("managed-environment:bill:sha256:v8-environment")
        );
        let desired_state = MigrationCutoverPlan::new(
            ProjectRecord::new(project_path.clone(), "bill".to_owned(), Vec::new()),
            target_environment,
        )
        .expect("environment cutover state");
        cutover_v7_migration(V7MigrationCutoverOptions {
            journal: &mut journal,
            plan: &plan,
            registry: &mut registry,
            desired_state: &desired_state,
            updated_at_unix_seconds: 40_003,
        })
        .await
        .expect("cut over environment");
        confirm_v7_migration(&mut journal, &plan, &mut registry, 40_004)
            .await
            .expect("confirm environment");

        assert_eq!(
            std::fs::read(&environment_path).expect("read user environment"),
            environment_bytes
        );
        std::fs::remove_dir_all(root).expect("remove adapter fixture");
    });
}

#[test]
fn v7_inventory_is_deterministic_complete_and_secret_free() {
    let mut database = service("db", Kind::Database, Driver::Postgres, "postgres:17");
    database.database = Some("bill".to_owned());
    database.username = Some("bill_user".to_owned());
    database.password = Some("database-secret".to_owned());
    database.env = Some(HashMap::from([
        ("Z_LAST".to_owned(), "environment-secret".to_owned()),
        ("A_FIRST".to_owned(), "visible-only-in-v7".to_owned()),
    ]));
    database.resolved_container_name = Some("bill-db".to_owned());

    let mut app = service(
        "app",
        Kind::App,
        Driver::Frankenphp,
        "ghcr.io/stackctl/php:8.4",
    );
    app.domain = Some("bill.test".to_owned());
    app.domains = Some(vec!["api.bill.test".to_owned(), "bill.test".to_owned()]);
    app.resolved_domain = Some("bill-random.test".to_owned());
    app.php_extensions = Some(vec!["redis".to_owned(), "intl".to_owned()]);
    app.trust_container_ca = true;
    app.hook.push(ServiceHook {
        name: "migrate".to_owned(),
        phase: HookPhase::PostUp,
        run: HookRun::Exec {
            argv: vec!["php".to_owned(), "artisan".to_owned(), "migrate".to_owned()],
        },
        on_error: HookOnError::Fail,
        timeout_sec: Some(60),
    });
    app.resolved_container_name = Some("bill-app".to_owned());

    let config = Config {
        schema_version: 7,
        project_type: ProjectType::Project,
        container_prefix: Some("bill".to_owned()),
        domain_strategy: None,
        service: vec![database, app],
        swarm: Vec::new(),
    };
    let observed = vec![
        observed_container("container-db", "bill-db", "db", "database"),
        observed_container("container-app", "bill-app", "app", "app"),
    ];

    let inventory = V7ProjectInventory::new(V7ProjectInventoryOptions {
        project_id: "bill",
        canonical_project_path: Path::new("/work/bill"),
        source_revision: &format!("sha256:{}", "a".repeat(64)),
        config: &config,
        observed_containers: &observed,
    })
    .expect("v7 inventory");

    assert_eq!(inventory.project_id(), "bill");
    assert_eq!(inventory.services()[0].service_id(), "app");
    assert_eq!(inventory.services()[1].service_id(), "db");
    assert_eq!(
        inventory.services()[1].observed_container_id(),
        Some("container-db")
    );
    assert_eq!(
        inventory.services()[1].observed_image_identity(),
        Some("sha256:container-db")
    );
    assert_eq!(inventory.services()[1].observed_mounts().len(), 1);
    assert_eq!(
        inventory.services()[1].observed_mounts()[0].source(),
        "bill-db-data"
    );
    assert_eq!(
        inventory.services()[1].credential_fields(),
        ["password", "username"]
    );
    assert_eq!(
        inventory.services()[1].environment_keys(),
        ["A_FIRST", "Z_LAST"]
    );
    assert_eq!(inventory.services()[1].volumes().len(), 1);
    assert_eq!(
        inventory.services()[1].volumes()[0].source(),
        &V7VolumeSource::Named("bill-db-data".to_owned())
    );
    assert_eq!(
        inventory.services()[1].volumes()[0].target(),
        "/var/lib/postgresql/data"
    );
    assert_eq!(
        inventory.services()[0].runtime_features(),
        [V7RuntimeFeature::Hooks, V7RuntimeFeature::PhpExtensions]
    );
    assert_eq!(
        inventory
            .routes()
            .iter()
            .map(|route| route.domain())
            .collect::<Vec<_>>(),
        ["api.bill.test", "bill-random.test", "bill.test"]
    );
    assert!(inventory.requires_legacy_ca_capture());
    assert!(inventory.blockers().is_empty());

    let debug = format!("{inventory:?}");
    assert!(!debug.contains("database-secret"));
    assert!(!debug.contains("environment-secret"));
    assert!(!debug.contains("visible-only-in-v7"));
    assert!(!debug.contains("artisan"));
}

#[test]
fn v7_inventory_rejects_ambiguous_container_ownership() {
    let mut database = service("db", Kind::Database, Driver::Postgres, "postgres:17");
    database.resolved_container_name = Some("bill-db".to_owned());
    let config = config(database);
    let observed = vec![
        observed_container("container-db-1", "bill-db", "db", "database"),
        observed_container("container-db-2", "bill-db", "db", "database"),
    ];

    let error =
        V7ProjectInventory::new(options(&config, &observed)).expect_err("ambiguous v7 ownership");

    assert_eq!(
        error.to_string(),
        "v7 service 'db' container 'bill-db' matches multiple Engine resources: container-db-1, container-db-2"
    );
}

#[test]
fn v7_inventory_preserves_unsupported_mounts_as_migration_blockers() {
    let mut database = service("db", Kind::Database, Driver::Postgres, "postgres:17");
    database.resolved_container_name = Some("bill-db".to_owned());
    database.volumes = Some(vec!["./database:/var/lib/postgresql/data".to_owned()]);
    let config = config(database);
    let observed = vec![
        observed_container("container-db", "bill-db", "db", "database").with_mounts(vec![
            ObservedContainerMount::new("./database", "/var/lib/postgresql/data", false, false),
        ]),
    ];

    let inventory = V7ProjectInventory::new(options(&config, &observed)).expect("v7 inventory");

    assert_eq!(
        inventory.services()[0].volumes()[0].source(),
        &V7VolumeSource::HostBind("./database".to_owned())
    );
    assert_eq!(
        inventory.blockers(),
        [V7InventoryBlocker::HostBindVolume {
            service_id: "db".to_owned(),
            source: "./database".to_owned(),
            target: "/var/lib/postgresql/data".to_owned(),
        }]
    );
}

#[test]
fn v7_inventory_blocks_observed_volume_drift() {
    let mut database = service("db", Kind::Database, Driver::Postgres, "postgres:17");
    database.resolved_container_name = Some("bill-db".to_owned());
    let config = config(database);
    let observed = vec![
        observed_container("container-db", "bill-db", "db", "database").with_mounts(vec![
            ObservedContainerMount::new("unexpected-data", "/var/lib/postgresql/data", true, false),
        ]),
    ];

    let inventory = V7ProjectInventory::new(options(&config, &observed)).expect("v7 inventory");

    assert_eq!(
        inventory.blockers(),
        [V7InventoryBlocker::VolumeMismatch {
            service_id: "db".to_owned(),
            target: "/var/lib/postgresql/data".to_owned(),
            expected_source: "bill-db-data".to_owned(),
            observed_source: Some("unexpected-data".to_owned()),
        }]
    );
    assert!(!inventory.ready_for_automatic_migration());
}

#[test]
fn v7_inventory_uses_the_typed_legacy_engine_discovery_boundary() {
    let mut database = service("db", Kind::Database, Driver::Postgres, "postgres:17");
    database.resolved_container_name = Some("bill-db".to_owned());
    let config = config(database);
    let discovery = RecordingLegacyDiscovery {
        observed: vec![observed_container(
            "container-db",
            "bill-db",
            "db",
            "database",
        )],
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("inventory runtime");

    let inventory = runtime
        .block_on(inventory_v7_project(
            &discovery,
            V7ProjectInventoryRequest {
                project_id: "bill",
                canonical_project_path: Path::new("/work/bill"),
                source_revision:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                config: &config,
            },
        ))
        .expect("discovered inventory");

    assert_eq!(
        inventory.services()[0].observed_container_id(),
        Some("container-db")
    );
}

struct RecordingLegacyDiscovery {
    observed: Vec<ObservedContainer>,
}

impl LegacyContainerDiscovery for RecordingLegacyDiscovery {
    fn discover_v7_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

fn options<'state>(
    config: &'state Config,
    observed_containers: &'state [ObservedContainer],
) -> V7ProjectInventoryOptions<'state> {
    V7ProjectInventoryOptions {
        project_id: "bill",
        canonical_project_path: Path::new("/work/bill"),
        source_revision: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        config,
        observed_containers,
    }
}

fn config(service: ServiceConfig) -> Config {
    Config {
        schema_version: 7,
        project_type: ProjectType::Project,
        container_prefix: Some("bill".to_owned()),
        domain_strategy: None,
        service: vec![service],
        swarm: Vec::new(),
    }
}

fn observed_container(id: &str, name: &str, service: &str, kind: &str) -> ObservedContainer {
    let observed = ObservedContainer::new(
        ContainerId::new(id),
        BTreeMap::from([
            ("com.stackctl.managed".to_owned(), "true".to_owned()),
            ("com.stackctl.container".to_owned(), name.to_owned()),
            ("com.stackctl.service".to_owned(), service.to_owned()),
            ("com.stackctl.kind".to_owned(), kind.to_owned()),
        ]),
    )
    .with_image_identity(format!("sha256:{id}"));
    if service == "db" {
        observed.with_mounts(vec![ObservedContainerMount::new(
            format!("{name}-data"),
            "/var/lib/postgresql/data",
            true,
            false,
        )])
    } else {
        observed
    }
}

fn service(name: &str, kind: Kind, driver: Driver, image: &str) -> ServiceConfig {
    ServiceConfig {
        name: name.to_owned(),
        kind,
        driver,
        image: image.to_owned(),
        host: "127.0.0.1".to_owned(),
        port: 0,
        database: None,
        username: None,
        password: None,
        bucket: None,
        access_key: None,
        secret_key: None,
        api_key: None,
        region: None,
        scheme: None,
        domain: None,
        domains: None,
        resolved_domain: None,
        container_port: None,
        smtp_port: None,
        volumes: None,
        env: None,
        command: None,
        depends_on: None,
        seed_file: None,
        hook: Vec::new(),
        health_path: None,
        health_statuses: None,
        restart: None,
        localhost_tls: false,
        octane: false,
        octane_workers: None,
        octane_max_requests: None,
        php_extensions: None,
        trust_container_ca: false,
        env_mapping: None,
        javascript: None,
        container_name: Some(format!("bill-{name}")),
        resolved_container_name: None,
    }
}

fn v7_execution_record(
    phase: V7MigrationExecutionPhase,
    checkpoints: Vec<V7MigrationAdapterCheckpoint>,
    updated_at_unix_seconds: i64,
) -> V7MigrationExecutionRecord {
    V7MigrationExecutionRecord::new(V7MigrationExecutionRecordOptions {
        project_id: "bill".to_owned(),
        canonical_project_path: "/work/bill".into(),
        evidence_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_owned(),
        adapter_plan_revision: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_owned(),
        phase,
        checkpoints,
        updated_at_unix_seconds,
    })
    .expect("v7 execution record")
}

fn lifecycle_v7_registry(
    calls: Arc<Mutex<Vec<String>>>,
    fail_route_cutover: bool,
) -> V7MigrationAdapterRegistry {
    let database: Box<dyn V7MigrationAdapterExecutor> = Box::new(RecordingV7Adapter {
        calls: Arc::clone(&calls),
        target_reference: Some("postgres-v8-bill"),
        fail_target_once: Arc::new(AtomicBool::new(false)),
        fail_cutover_once: Arc::new(AtomicBool::new(false)),
    });
    let route: Box<dyn V7MigrationAdapterExecutor> = Box::new(RecordingV7Adapter {
        calls,
        target_reference: None,
        fail_target_once: Arc::new(AtomicBool::new(false)),
        fail_cutover_once: Arc::new(AtomicBool::new(fail_route_cutover)),
    });

    let mut registry = V7MigrationAdapterRegistry::default();
    registry
        .register("service/database", "postgres-logical-database", database)
        .expect("register database adapter");
    registry
        .register("route", "no-routes", route)
        .expect("register route adapter");

    registry
}

#[derive(Clone, Default)]
struct RecordingV7Journal {
    current: Option<V7MigrationExecutionRecord>,
    writes: Vec<V7MigrationExecutionRecord>,
}

impl V7MigrationExecutionJournal for RecordingV7Journal {
    fn load_v7_execution(
        &self,
        _canonical_project_path: &Path,
        _evidence_revision: &str,
    ) -> Result<Option<V7MigrationExecutionRecord>, StateStoreError> {
        Ok(self.current.clone())
    }

    fn persist_v7_execution(
        &mut self,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError> {
        self.current = Some(execution.clone());
        self.writes.push(execution.clone());

        Ok(())
    }

    fn persist_v7_cutover(
        &mut self,
        desired_state: &MigrationCutoverPlan,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError> {
        assert_eq!(
            desired_state.project().project_name(),
            execution.project_id()
        );
        assert_eq!(
            desired_state.project().canonical_path(),
            execution.canonical_project_path()
        );
        assert_eq!(
            desired_state.environment().project_id(),
            execution.project_id()
        );
        self.persist_v7_execution(execution)
    }

    fn persist_v7_rollback(
        &mut self,
        restored_state: &MigrationRollbackPlan,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError> {
        assert_eq!(
            restored_state.project().project_name(),
            execution.project_id()
        );
        assert_eq!(
            restored_state.project().canonical_path(),
            execution.canonical_project_path()
        );
        assert_eq!(
            restored_state.environment().project_id(),
            execution.project_id()
        );
        self.persist_v7_execution(execution)
    }
}

fn v7_cutover_state() -> MigrationCutoverPlan {
    v7_cutover_state_at("/work/bill")
}

fn v7_cutover_state_at(canonical_path: &str) -> MigrationCutoverPlan {
    MigrationCutoverPlan::new(
        ProjectRecord::new(
            canonical_path.into(),
            "bill".to_owned(),
            vec!["bill-app.stackctl.localhost".to_owned()],
        ),
        ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
            project_id: "bill".to_owned(),
            revision: "sha256:v8-environment".to_owned(),
            values: BTreeMap::new(),
            lifecycle: EnvironmentLifecycle::Active,
        }),
    )
    .expect("v7 cutover state")
}

fn v7_rollback_state() -> MigrationRollbackPlan {
    MigrationRollbackPlan::new(
        ProjectRecord::new(
            "/work/bill".into(),
            "bill".to_owned(),
            vec!["bill-legacy.stackctl.localhost".to_owned()],
        ),
        ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
            project_id: "bill".to_owned(),
            revision: "sha256:v7-environment".to_owned(),
            values: BTreeMap::new(),
            lifecycle: EnvironmentLifecycle::Active,
        }),
        Vec::new(),
    )
    .expect("v7 rollback state")
}

struct RecordingV7Adapter {
    calls: Arc<Mutex<Vec<String>>>,
    target_reference: Option<&'static str>,
    fail_target_once: Arc<AtomicBool>,
    fail_cutover_once: Arc<AtomicBool>,
}

impl V7MigrationAdapterExecutor for RecordingV7Adapter {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("recovery:{}", checkpoint.adapter_id()));
        let reference = format!("backup:{}", checkpoint.adapter_id());
        Box::pin(async move {
            MigrationBackup::new(
                reference,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                100,
            )
        })
    }

    fn prepare_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("target:{}", checkpoint.adapter_id()));
        let fail = self.fail_target_once.swap(false, Ordering::SeqCst);
        let target_reference = self.target_reference;
        Box::pin(async move {
            if fail {
                return Err(MigrationOperationError::new("target unavailable"));
            }
            match target_reference {
                Some(reference) => V7MigrationAdapterTarget::resource(reference)
                    .map_err(MigrationOperationError::new),
                None => Ok(V7MigrationAdapterTarget::NoExternalTarget),
            }
        })
    }

    fn cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("cutover:{}", checkpoint.adapter_id()));
        let fail = self.fail_cutover_once.swap(false, Ordering::SeqCst);
        Box::pin(async move {
            if fail {
                Err(MigrationOperationError::new("cutover unavailable"))
            } else {
                Ok(())
            }
        })
    }

    fn rollback<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("rollback:{}", checkpoint.adapter_id()));
        Box::pin(async { Ok(()) })
    }

    fn confirm<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("confirm:{}", checkpoint.adapter_id()));
        Box::pin(async { Ok(()) })
    }
}

fn run_test(test: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(test);
}
