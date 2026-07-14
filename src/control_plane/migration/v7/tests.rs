use super::{
    V7EnvironmentMigrationAdapter, V7GeneratedEnvironmentRollbackOptions,
    V7HostArtifactDiscoveryOptions, V7InventoryBlocker, V7MigrationAdapterSelectionOptions,
    V7MigrationRouteSource, V7MigrationServiceAdapter, V7MigrationServiceSource,
    V7ProjectInventory, V7ProjectInventoryOptions, V7ProjectInventoryRequest,
    V7RouteMigrationAdapter, V7RuntimeFeature, V7TrustMigrationAdapter, V7VolumeMigrationAdapter,
    V7VolumeSource, capture_v7_generated_environment_rollback, inventory_v7_host_artifacts,
    inventory_v7_project, read_v7_generated_environment_rollback, select_v7_migration_adapters,
};
use crate::config::{
    Config, Driver, HookOnError, HookPhase, HookRun, Kind, ProjectType, ServiceConfig, ServiceHook,
};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::engine::{
    ContainerId, EngineFuture, LegacyContainerDiscovery, ObservedContainer, ObservedContainerMount,
};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

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
