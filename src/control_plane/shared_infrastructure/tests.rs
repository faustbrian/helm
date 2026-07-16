use super::{
    CompatibilityFingerprint, CompatibilityFingerprintOptions, CompatibilityProfile,
    CredentialEntropy, CredentialGenerationError, CredentialSecret, GotenbergSharedInstancePlan,
    GotenbergSharedInstancePlanOptions, IsolationCapability, MailpitAuthenticationSnapshot,
    MailpitProjectDefinition, MailpitSharedInstancePlan, MailpitSharedInstancePlanOptions,
    MongoDbLogicalResourcePlan, MongoDbMigrationInstancePlanOptions,
    MongoDbMigrationPreparationOptions, MongoDbSharedInstancePlan,
    MongoDbSharedInstancePlanOptions, MySqlFlavor, MySqlMigrationInstancePlanOptions,
    MySqlMigrationPreparationOptions, MySqlSharedInstancePlan, MySqlSharedInstancePlanOptions,
    ObjectStoreFlavor, ObjectStoreProjectResources, ObjectStoreSharedInstancePlan,
    ObjectStoreSharedInstancePlanOptions, OrphanedSharedAccessOptions, PersistenceMode,
    PostgresLogicalResourcePlan, PostgresMigrationInstancePlanOptions,
    PostgresMigrationPreparationOptions, PostgresPreparationOptions, PostgresSharedInstancePlan,
    PostgresSharedInstancePlanOptions, PreparedPostgresSharedInstance, ProvisioningJobOptions,
    ProvisioningJobsRunOptions, RabbitMqDefinitions, RabbitMqPasswordHash,
    RabbitMqProjectDefinition, RabbitMqSharedInstancePlan, RabbitMqSharedInstancePlanOptions,
    RedisAclProject, RedisAclSnapshot, RedisFlavor, RedisSharedInstancePlan,
    RedisSharedInstancePlanOptions, SharedInfrastructureReconcileError, SharedPreparationOptions,
    SharedServiceReconcileAction, SharedServiceReconcileOptions, SharedServiceRequest,
    SharedVolumeReconcileAction, SharedVolumeReconcileOptions,
    SqlServerMigrationInstancePlanOptions, SqlServerMigrationPreparationOptions,
    SqlServerSharedInstancePlan, SqlServerSharedInstancePlanOptions,
    UnreferencedSharedServiceOptions, generate_credential_secret, plan_gotenberg_project_resources,
    plan_mailpit_project_resources, plan_mongodb_project_resources, plan_mysql_project_resources,
    plan_object_store_project_resources, plan_postgres_project_resources,
    plan_rabbitmq_project_resources, plan_redis_project_resources, plan_shared_instances,
    plan_sql_server_project_resources, prepare_mongodb_migration_target,
    prepare_mongodb_shared_instances, prepare_mysql_migration_target,
    prepare_mysql_shared_instances, prepare_postgres_migration_target,
    prepare_postgres_shared_instances, prepare_shared_instances,
    prepare_sql_server_migration_target, provision_mongodb_logical_resource,
    provision_mysql_logical_resource, provision_object_store_project_resources,
    provision_postgres_logical_resource, provision_sql_server_logical_resource,
    reconcile_mailpit_authentication, reconcile_mongodb_migration_target,
    reconcile_mongodb_project_resources, reconcile_mysql_migration_target,
    reconcile_mysql_project_resources, reconcile_object_store_project_resources,
    reconcile_postgres_migration_target, reconcile_postgres_project_resources,
    reconcile_prepared_mongodb_instance, reconcile_prepared_mysql_instance,
    reconcile_prepared_postgres_instance, reconcile_prepared_shared_instance,
    reconcile_rabbitmq_definitions, reconcile_redis_acl_snapshot, reconcile_shared_service,
    reconcile_shared_volume, reconcile_sql_server_migration_target,
    reconcile_sql_server_project_resources, reload_rabbitmq_definitions, reload_redis_acl,
    resolve_execution_shared_instances, revoke_orphaned_shared_access,
    revoke_orphaned_shared_access_from_observed, revoke_rabbitmq_project_access,
    run_provisioning_job, run_provisioning_job_from_observed, run_provisioning_jobs_from_observed,
    shared_identity_hex, stop_unreferenced_shared_services,
    stop_unreferenced_shared_services_from_observed, store_credential_secret,
    store_mailpit_authentication, store_rabbitmq_definitions, store_redis_acl_snapshot,
    wait_for_mongodb_readiness,
};
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, ObservedContainer, OwnedContainer,
    reconstruct_owned_container,
};
use crate::control_plane::resolve_execution_plan;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, ResourceLifecycle, ResourceRecord, ResourceRecordOptions,
    ResourceRetention, SqliteStateStore, StateStore,
};
use futures_util::stream;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn shared_postgres_source(
    directory: &str,
    project: &str,
    version: &str,
    image: &str,
) -> ProjectSource {
    ProjectSource::new(
        PathBuf::from(directory),
        PathBuf::from(format!("{directory}/.stackctl.yaml")),
        format!(
            "schema_version: 8\nproject: {project}\nservices:\n  db:\n    preset: postgres\n    version: \"{version}\"\n    image: {image}\n"
        ),
    )
}

fn shared_database_source(
    directory: &str,
    project: &str,
    preset: &str,
    version: &str,
    image: &str,
) -> ProjectSource {
    ProjectSource::new(
        PathBuf::from(directory),
        PathBuf::from(format!("{directory}/.stackctl.yaml")),
        format!(
            "schema_version: 8\nproject: {project}\nservices:\n  db:\n    preset: {preset}\n    version: \"{version}\"\n    image: {image}\n"
        ),
    )
}

fn shared_preset_source(
    directory: &str,
    project: &str,
    service: &str,
    preset: &str,
    version: &str,
    image: &str,
) -> ProjectSource {
    ProjectSource::new(
        PathBuf::from(directory),
        PathBuf::from(format!("{directory}/.stackctl.yaml")),
        format!(
            "schema_version: 8\nproject: {project}\nservices:\n  {service}:\n    preset: {preset}\n    version: \"{version}\"\n    image: {image}\n"
        ),
    )
}

fn shared_sql_server_source(directory: &str, project: &str, image: &str) -> ProjectSource {
    ProjectSource::new(
        PathBuf::from(directory),
        PathBuf::from(format!("{directory}/.stackctl.yaml")),
        format!(
            "schema_version: 8\nproject: {project}\nservices:\n  db:\n    preset: sqlserver\n    version: \"2022\"\n    image: {image}\n    environment:\n      ACCEPT_EULA: \"Y\"\n"
        ),
    )
}

struct FixedCredentialEntropy(u8);

impl CredentialEntropy for FixedCredentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        bytes.fill(self.0);

        Ok(())
    }
}

#[test]
fn equivalent_postgres_profiles_share_one_fingerprint() {
    let first = fingerprint(vec!["postgis", "pg_stat_statements"], "17");
    let second = fingerprint(vec!["pg_stat_statements", "postgis"], "17");

    assert_eq!(first, second);
    assert_eq!(first.as_str().len(), 71);
    assert!(first.as_str().starts_with("sha256:"));
}

#[test]
fn exact_postgres_demands_share_one_execution_instance() {
    let image = concat!(
        "postgres@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_postgres_source("/work/bill", "bill", "17", image),
        shared_postgres_source("/work/shop", "shop", "17", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");

    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].profile().implementation(), "postgresql");
    assert_eq!(shared[0].profile().major_version(), "17");
    assert_eq!(shared[0].consumers().len(), 2);
    assert_eq!(shared[0].consumers()[0].project_id(), "bill");
    assert_eq!(shared[0].consumers()[1].project_id(), "shop");
}

#[test]
fn postgres_version_or_image_differences_never_share_an_instance() {
    let first = concat!(
        "postgres@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let second = concat!(
        "postgres@sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    let registry = plan_project_registry(&[
        shared_postgres_source("/work/bill", "bill", "17", first),
        shared_postgres_source("/work/shop", "shop", "18", first),
        shared_postgres_source("/work/portal", "portal", "17", second),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");

    assert_eq!(shared.len(), 3);
}

#[test]
fn mysql_and_mariadb_demands_resolve_as_distinct_compatibility_strategies() {
    let mysql = concat!(
        "mysql@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let mariadb = concat!(
        "mariadb@sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    let registry = plan_project_registry(&[
        shared_database_source("/work/bill", "bill", "mysql", "8", mysql),
        shared_database_source("/work/shop", "shop", "mysql", "8", mysql),
        shared_database_source("/work/portal", "portal", "mariadb", "11", mariadb),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");

    assert_eq!(shared.len(), 2);
    assert_eq!(shared[0].profile().implementation(), "mysql");
    assert_eq!(shared[0].consumers().len(), 2);
    assert_eq!(shared[1].profile().implementation(), "mariadb");
    assert_eq!(shared[1].consumers().len(), 1);
}

#[test]
fn mysql_strategy_prepares_and_reconciles_one_instance_for_two_projects() {
    let image = concat!(
        "mysql@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_database_source("/work/bill", "bill", "mysql", "8", image),
        shared_database_source("/work/shop", "shop", "mysql", "8", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-mysql-strategy-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let first = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: database_path.parent().expect("state directory"),
        },
    )
    .expect("shared preparation");
    let first_password = first[0].environments()[0]
        .values()
        .get("DB_PASSWORD")
        .expect("first project password")
        .clone();
    let prepared = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: database_path.parent().expect("state directory"),
        },
    )
    .expect("replayed shared preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_shared_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("shared reconciliation");

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].service_identities().len(), 2);
    assert_eq!(prepared[0].environments().len(), 2);
    assert_eq!(
        prepared[0].environments()[0]
            .values()
            .get("DB_PASSWORD")
            .expect("replayed project password"),
        &first_password
    );
    assert_eq!(store.credentials().expect("durable credentials").len(), 3);
    assert_eq!(result.physical_resources().len(), 2);
    assert_eq!(result.logical_resources().len(), 2);
    assert_eq!(engine.created_containers.len(), 1);
    let commands = engine.command_arguments.lock().expect("commands");
    assert_eq!(commands.len(), 3);
    assert_eq!(
        commands[0].last().map(String::as_str),
        Some("--execute=SELECT 1")
    );
    assert!(commands[1..].iter().all(|arguments| arguments.len() == 5));
    drop(commands);

    drop(store);
    std::fs::remove_file(database_path).expect("remove state store");
}

#[cfg(unix)]
#[test]
fn mongodb_strategy_prepares_and_reconciles_one_instance_for_two_projects() {
    let image = concat!(
        "mongo@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_database_source("/work/bill", "bill", "mongodb", "8", image),
        shared_database_source("/work/shop", "shop", "mongodb", "8", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let root = std::env::temp_dir().join(format!(
        "stackctl-mongodb-strategy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("strategy state directory");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let first = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("shared preparation");
    let first_password = first[0].environments()[0]
        .values()
        .get("MONGODB_PASSWORD")
        .expect("first project password")
        .clone();
    let prepared = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("replayed shared preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_shared_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("shared reconciliation");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(shared[0].profile().implementation(), "mongodb");
    assert_eq!(prepared[0].service_identities().len(), 2);
    assert_eq!(prepared[0].environments().len(), 2);
    assert_eq!(
        prepared[0].environments()[0]
            .values()
            .get("MONGODB_PASSWORD")
            .expect("replayed project password"),
        &first_password
    );
    let credentials = store.credentials().expect("durable credentials");
    assert_eq!(credentials.len(), 3);
    assert_eq!(result.physical_resources().len(), 2);
    assert_eq!(result.logical_resources().len(), 2);
    assert_eq!(engine.created_containers.len(), 1);
    let commands = engine.command_arguments.lock().expect("commands");
    assert_eq!(commands.len(), 3);
    assert!(commands.iter().all(|arguments| {
        arguments.iter().map(String::as_str).collect::<Vec<_>>() == ["mongosh", "--quiet", "--nodb"]
    }));
    drop(commands);
    let command_inputs = engine.command_inputs.lock().expect("command inputs");
    assert_eq!(command_inputs.len(), 3);
    let readiness = std::str::from_utf8(&command_inputs[0]).expect("readiness script UTF-8");
    assert!(readiness.contains("runCommand({ ping: 1 })"));
    assert!(!readiness.contains("createUser"));
    assert!(command_inputs[1..].iter().all(|input| {
        std::str::from_utf8(input)
            .expect("tenant script UTF-8")
            .contains("createUser")
    }));
    drop(command_inputs);
    let bootstrap = credentials
        .iter()
        .find(|credential| credential.project_id().is_none())
        .expect("bootstrap credential");
    assert_eq!(
        engine.created_containers[0]
            .environment()
            .get("MONGO_INITDB_ROOT_PASSWORD")
            .map(String::as_str),
        Some(bootstrap.secret())
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove strategy state");
}

#[test]
fn sql_server_strategy_requires_explicit_eula_acceptance() {
    let image = concat!(
        "mcr.microsoft.com/mssql/server@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[shared_database_source(
        "/work/bill",
        "bill",
        "sqlserver",
        "2022",
        image,
    )])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    let error = resolve_execution_shared_instances(&execution, "linux/amd64")
        .expect_err("missing EULA acceptance");

    assert_eq!(
        error.to_string(),
        "shared SQL Server service 'bill-db' requires environment.ACCEPT_EULA: \"Y\""
    );
}

#[test]
fn sql_server_strategy_prepares_and_reconciles_one_instance_for_two_projects() {
    let image = concat!(
        "mcr.microsoft.com/mssql/server@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_sql_server_source("/work/bill", "bill", image),
        shared_sql_server_source("/work/shop", "shop", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/amd64")
        .expect("shared execution instances");
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-sqlserver-strategy-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let first = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: database_path.parent().expect("state directory"),
        },
    )
    .expect("shared preparation");
    let first_password = first[0].environments()[0]
        .values()
        .get("DB_PASSWORD")
        .expect("first project password")
        .clone();
    let prepared = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: database_path.parent().expect("state directory"),
        },
    )
    .expect("replayed shared preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_shared_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("shared reconciliation");

    assert_eq!(shared[0].profile().implementation(), "sqlserver");
    assert_eq!(prepared[0].service_identities().len(), 2);
    assert_eq!(prepared[0].environments().len(), 2);
    assert_eq!(
        prepared[0].environments()[0]
            .values()
            .get("DB_PASSWORD")
            .expect("replayed project password"),
        &first_password
    );
    assert!(first_password.starts_with("St1"));
    assert_eq!(store.credentials().expect("durable credentials").len(), 3);
    assert_eq!(result.physical_resources().len(), 2);
    assert_eq!(result.logical_resources().len(), 2);
    assert_eq!(engine.created_containers.len(), 1);
    assert_eq!(engine.command_arguments.lock().expect("commands").len(), 2);

    drop(store);
    std::fs::remove_file(database_path).expect("remove state store");
}

#[test]
fn gotenberg_strategy_shares_one_stateless_endpoint_for_two_projects() {
    let image = concat!(
        "gotenberg/gotenberg@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_preset_source("/work/bill", "bill", "gotenberg", "gotenberg", "8", image),
        shared_preset_source("/work/shop", "shop", "gotenberg", "gotenberg", "8", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-gotenberg-strategy-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let prepared = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: database_path.parent().expect("state directory"),
        },
    )
    .expect("shared preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_shared_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("shared reconciliation");

    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].profile().implementation(), "gotenberg");
    assert_eq!(prepared[0].service_identities().len(), 2);
    assert!(prepared[0].credential_service_identities().is_empty());
    assert_eq!(prepared[0].environments().len(), 2);
    assert_eq!(
        prepared[0].environments()[0].values().get("GOTENBERG_URL"),
        Some(&format!(
            "http://{}:3000",
            engine
                .created_containers
                .first()
                .map(|options| options.name())
                .unwrap_or("not-created-yet")
        ))
    );
    assert!(store.credentials().expect("durable credentials").is_empty());
    assert_eq!(result.physical_resources().len(), 1);
    assert_eq!(result.logical_resources().len(), 2);
    assert_eq!(engine.created_containers.len(), 1);
    assert!(
        engine
            .command_arguments
            .lock()
            .expect("commands")
            .is_empty()
    );

    drop(store);
    std::fs::remove_file(database_path).expect("remove state store");
}

#[cfg(unix)]
#[test]
fn redis_strategy_publishes_one_acl_snapshot_for_two_projects() {
    let image = concat!(
        "redis@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_database_source("/work/bill", "bill", "redis", "8", image),
        shared_database_source("/work/shop", "shop", "redis", "8", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let root = std::env::temp_dir().join(format!(
        "stackctl-redis-strategy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("strategy state directory");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let first = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("shared preparation");
    let first_password = first[0].environments()[0]
        .values()
        .get("REDIS_PASSWORD")
        .expect("first project password")
        .clone();
    let prepared = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("replayed shared preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_shared_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("shared reconciliation");

    assert_eq!(shared[0].profile().implementation(), "redis");
    assert_eq!(prepared[0].service_identities().len(), 2);
    assert_eq!(prepared[0].credential_service_identities().len(), 2);
    assert_eq!(
        prepared[0].environments()[0]
            .values()
            .get("REDIS_PASSWORD")
            .expect("replayed project password"),
        &first_password
    );
    assert_eq!(store.credentials().expect("durable credentials").len(), 3);
    assert_eq!(result.physical_resources().len(), 2);
    assert_eq!(result.logical_resources().len(), 2);
    assert_eq!(engine.created_containers.len(), 1);
    let commands = engine.command_arguments.lock().expect("commands");
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].last().map(String::as_str), Some("PING"));
    assert_eq!(
        commands[1].iter().map(String::as_str).collect::<Vec<_>>(),
        ["redis-cli", "-e", "--user", "stackctl_admin", "ACL", "LOAD"]
    );
    drop(commands);
    let identity = shared_identity_hex(
        shared[0]
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .expect("fingerprint identity"),
    );
    let acl = root
        .join("shared")
        .join("install-1")
        .join(identity)
        .join("redis-acl/mounted/users.acl");
    let contents = std::fs::read_to_string(acl).expect("ACL contents");
    assert!(contents.contains("st_bill_db"));
    assert!(contents.contains("st_shop_db"));
    assert!(!contents.contains(&first_password));

    drop(store);
    std::fs::remove_dir_all(root).expect("remove strategy state");
}

#[cfg(unix)]
#[test]
fn minio_strategy_publishes_isolated_policies_for_two_projects() {
    let image = concat!(
        "minio/minio@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_database_source("/work/bill", "bill", "minio", "1", image),
        shared_database_source("/work/shop", "shop", "minio", "1", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let root = std::env::temp_dir().join(format!(
        "stackctl-minio-strategy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("strategy state directory");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let first = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("shared preparation");
    let first_password = first[0].environments()[0]
        .values()
        .get("AWS_SECRET_ACCESS_KEY")
        .expect("first project secret")
        .clone();
    let prepared = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("replayed shared preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_shared_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("shared reconciliation");

    assert_eq!(shared[0].profile().implementation(), "minio");
    assert_eq!(prepared[0].service_identities().len(), 2);
    assert_eq!(prepared[0].environments().len(), 2);
    assert_eq!(
        prepared[0].environments()[0]
            .values()
            .get("AWS_SECRET_ACCESS_KEY")
            .expect("replayed project secret"),
        &first_password
    );
    assert_eq!(store.credentials().expect("durable credentials").len(), 3);
    assert_eq!(result.physical_resources().len(), 2);
    assert_eq!(result.logical_resources().len(), 2);
    assert_eq!(engine.created_containers.len(), 1);
    assert_eq!(engine.command_arguments.lock().expect("commands").len(), 8);
    let identity = shared_identity_hex(
        shared[0]
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .expect("fingerprint identity"),
    );
    let policies = root
        .join("shared")
        .join("install-1")
        .join(identity)
        .join("object-store-policies");
    let bill_policy =
        std::fs::read_to_string(policies.join("stackctl-bill-db.json")).expect("bill policy");
    let shop_policy =
        std::fs::read_to_string(policies.join("stackctl-shop-db.json")).expect("shop policy");
    assert!(bill_policy.contains("arn:aws:s3:::stackctl-bill-db/*"));
    assert!(shop_policy.contains("arn:aws:s3:::stackctl-shop-db/*"));
    assert!(!bill_policy.contains("stackctl-shop-db"));
    assert!(!shop_policy.contains("stackctl-bill-db"));
    assert!(!bill_policy.contains(&first_password));
    assert!(!shop_policy.contains(&first_password));

    drop(store);
    std::fs::remove_dir_all(root).expect("remove strategy state");
}

#[cfg(unix)]
#[test]
fn rabbitmq_strategy_publishes_isolated_vhosts_for_two_projects() {
    let image = concat!(
        "rabbitmq@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_database_source("/work/bill", "bill", "rabbitmq", "4", image),
        shared_database_source("/work/shop", "shop", "rabbitmq", "4", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let root = std::env::temp_dir().join(format!(
        "stackctl-rabbitmq-strategy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("strategy state directory");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let first = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("shared preparation");
    let first_password = first[0].environments()[0]
        .values()
        .get("RABBITMQ_PASSWORD")
        .expect("first project password")
        .clone();
    let prepared = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("replayed shared preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_shared_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("shared reconciliation");

    assert_eq!(shared[0].profile().implementation(), "rabbitmq");
    assert_eq!(prepared[0].service_identities().len(), 2);
    assert_eq!(prepared[0].environments().len(), 2);
    assert_eq!(
        prepared[0].environments()[0]
            .values()
            .get("RABBITMQ_PASSWORD")
            .expect("replayed project password"),
        &first_password
    );
    assert_eq!(store.credentials().expect("durable credentials").len(), 2);
    assert_eq!(result.physical_resources().len(), 2);
    assert_eq!(result.logical_resources().len(), 2);
    assert_eq!(engine.created_containers.len(), 1);
    assert_eq!(
        *engine.command_arguments.lock().expect("commands"),
        vec![
            vec![
                "rabbitmq-diagnostics".to_owned(),
                "-q".to_owned(),
                "check_running".to_owned(),
            ],
            vec![
                "rabbitmqctl".to_owned(),
                "import_definitions".to_owned(),
                "/etc/stackctl/rabbitmq/definitions.json".to_owned(),
            ],
        ]
    );
    assert_eq!(
        *engine
            .reconnected_networks
            .lock()
            .expect("reconnected networks"),
        [(
            "created-shared-service".to_owned(),
            "network-1".to_owned(),
            engine.created_containers[0].name().to_owned(),
        )]
    );
    let identity = shared_identity_hex(
        shared[0]
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .expect("fingerprint identity"),
    );
    let definitions = std::fs::read_to_string(
        root.join("shared")
            .join("install-1")
            .join(identity)
            .join("rabbitmq-definitions/mounted/definitions.json"),
    )
    .expect("RabbitMQ definitions");
    assert!(definitions.contains("stackctl_bill_db"));
    assert!(definitions.contains("stackctl_shop_db"));
    assert!(definitions.contains("st_bill_db"));
    assert!(definitions.contains("st_shop_db"));
    assert!(!definitions.contains(&first_password));

    drop(store);
    std::fs::remove_dir_all(root).expect("remove strategy state");
}

#[cfg(unix)]
#[test]
fn mailpit_strategy_attributes_smtp_and_ui_access_for_two_projects() {
    let image = concat!(
        "axllent/mailpit@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_preset_source("/work/bill", "bill", "mailpit", "mailpit", "1", image),
        shared_preset_source("/work/shop", "shop", "mailpit", "mailpit", "1", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let root = std::env::temp_dir().join(format!(
        "stackctl-mailpit-strategy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("strategy state directory");
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let first = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("shared preparation");
    let first_password = first[0].environments()[0]
        .values()
        .get("MAIL_PASSWORD")
        .expect("first project password")
        .clone();
    let prepared = prepare_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        SharedPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
            state_directory: &root,
        },
    )
    .expect("replayed shared preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_shared_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("shared reconciliation");

    assert_eq!(shared[0].profile().implementation(), "mailpit");
    assert_eq!(prepared[0].service_identities().len(), 2);
    assert_eq!(prepared[0].environments().len(), 2);
    assert_eq!(prepared[0].routes().len(), 2);
    assert_eq!(
        prepared[0].routes()[0].domain(),
        "bill-mailpit.stackctl.localhost"
    );
    assert_eq!(
        prepared[0].routes()[1].domain(),
        "shop-mailpit.stackctl.localhost"
    );
    assert_eq!(
        prepared[0].environments()[0]
            .values()
            .get("MAIL_PASSWORD")
            .expect("replayed project password"),
        &first_password
    );
    assert_eq!(store.credentials().expect("durable credentials").len(), 2);
    assert_eq!(result.physical_resources().len(), 2);
    assert_eq!(result.logical_resources().len(), 2);
    assert_eq!(engine.created_containers.len(), 1);
    assert!(
        engine
            .command_arguments
            .lock()
            .expect("commands")
            .is_empty()
    );
    let identity = shared_identity_hex(
        shared[0]
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .expect("fingerprint identity"),
    );
    let authentication = std::fs::read_to_string(
        root.join("shared")
            .join("install-1")
            .join(&identity)
            .join("mailpit-authentication/mounted/smtp-passwords"),
    )
    .expect("Mailpit authentication");
    assert!(authentication.contains("st_bill_mailpit:$2b$"));
    assert!(authentication.contains("st_shop_mailpit:$2b$"));
    assert!(!authentication.contains(&first_password));

    drop(store);
    std::fs::remove_dir_all(root).expect("remove strategy state");
}

#[test]
fn postgres_preparation_reuses_durable_bootstrap_and_project_secrets() {
    let image = concat!(
        "postgres@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_postgres_source("/work/bill", "bill", "17", image),
        shared_postgres_source("/work/shop", "shop", "17", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-postgres-preparation-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");

    let first = prepare_postgres_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        PostgresPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
        },
    )
    .expect("first PostgreSQL preparation");
    let second = prepare_postgres_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        PostgresPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
        },
    )
    .expect("replayed PostgreSQL preparation");

    assert_eq!(first.len(), 1);
    assert_eq!(first[0].projects().len(), 2);
    assert_eq!(
        first[0].instance().bootstrap_credential().secret(),
        second[0].instance().bootstrap_credential().secret()
    );
    assert_eq!(
        first[0].projects()[0].credential().secret(),
        second[0].projects()[0].credential().secret()
    );
    assert_eq!(store.credentials().expect("durable credentials").len(), 3);

    drop(store);
    std::fs::remove_file(database_path).expect("remove state store");
}

#[test]
fn prepared_postgres_reconciles_one_process_and_every_project_tenant() {
    let image = concat!(
        "postgres@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let registry = plan_project_registry(&[
        shared_postgres_source("/work/bill", "bill", "17", image),
        shared_postgres_source("/work/shop", "shop", "17", image),
    ])
    .expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
        .expect("shared execution instances");
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-postgres-reconciliation-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let prepared = prepare_postgres_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        PostgresPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
        },
    )
    .expect("PostgreSQL preparation");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let logical = runtime
        .block_on(reconcile_prepared_postgres_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("prepared PostgreSQL reconciliation");

    assert_eq!(logical.physical_resources().len(), 2);
    assert_eq!(logical.physical_resources()[0].kind(), "shared_service");
    assert_eq!(logical.physical_resources()[1].kind(), "volume");
    assert_eq!(
        logical.physical_resources()[0].scope_id(),
        Some(prepared[0].instance().container().name())
    );
    assert_eq!(
        logical.physical_resources()[1].scope_id(),
        Some(
            prepared[0]
                .instance()
                .volume()
                .expect("persistent PostgreSQL volume")
                .metadata()
                .resource_id()
                .expect("volume scope")
        )
    );
    assert_eq!(logical.logical_resources().len(), 2);
    assert!(logical.logical_resource_drifts().is_empty());
    assert_eq!(logical.logical_resources()[0].project_id(), "bill");
    assert_eq!(logical.logical_resources()[1].project_id(), "shop");
    assert_eq!(
        logical.logical_resources()[0].shared_resource_id(),
        prepared[0]
            .instance()
            .volume()
            .expect("persistent PostgreSQL volume")
            .name()
    );
    assert_eq!(engine.created_containers.len(), 1);
    assert_eq!(engine.started_containers.len(), 1);
    let commands = engine.command_arguments.lock().expect("commands");
    assert_eq!(commands.len(), 3);
    assert_eq!(
        commands[0].last().map(String::as_str),
        Some("--command=SELECT 1")
    );
    assert!(
        commands[1..]
            .iter()
            .all(|arguments| arguments.last().map(String::as_str) == Some("--dbname=postgres"))
    );
    drop(commands);

    let mut partial_engine = RecordingSharedVolumeEngine::default();
    partial_engine
        .command_exits
        .lock()
        .expect("command exit queue")
        .extend([0, 1, 0]);
    let partial = runtime
        .block_on(reconcile_prepared_postgres_instance(
            &mut partial_engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("isolated PostgreSQL tenant drift");

    assert_eq!(partial.physical_resources().len(), 2);
    assert_eq!(partial.logical_resources().len(), 1);
    assert_eq!(partial.logical_resources()[0].project_id(), "shop");
    assert_eq!(
        partial.logical_resource_drifts()[0].resource_id(),
        "stackctl_bill_db"
    );
    assert_eq!(
        partial_engine
            .command_arguments
            .lock()
            .expect("partial commands")
            .len(),
        3
    );

    drop(store);
    std::fs::remove_file(database_path).expect("remove state store");
}

#[test]
fn missing_shared_persistent_volumes_are_created_once() {
    let request = shared_volume_request();
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_shared_volume(
            &mut engine,
            SharedVolumeReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("create shared volume");

    assert_eq!(result.action(), SharedVolumeReconcileAction::Created);
    assert_eq!(result.volume().name(), request.name());
    assert_eq!(engine.created, vec![request]);
    assert!(engine.removed.is_empty());
}

#[test]
fn provisioning_jobs_are_bounded_owned_and_removed_after_success() {
    let request = provisioning_job_request("install-1");
    let mut engine = RecordingSharedVolumeEngine::default();
    let completions = Arc::clone(&engine.completions);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    runtime
        .block_on(run_provisioning_job(
            &mut engine,
            ProvisioningJobOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(30),
            },
        ))
        .expect("run provisioning job");

    assert_eq!(engine.created_containers, vec![request.clone()]);
    assert_eq!(engine.started_containers.len(), 1);
    assert_eq!(engine.removed_containers.len(), 1);
    assert_eq!(engine.ensured_images, [request.image()]);
    assert_eq!(*completions.lock().expect("completion count"), 1);
    assert_eq!(
        engine.operations,
        vec![
            "ensure-image",
            "create-container",
            "start-container",
            "remove-container"
        ]
    );
}

#[test]
fn independent_provisioning_jobs_are_bounded_and_preserve_request_order() {
    let requests = [
        provisioning_job_request_for("object-store-bucket"),
        provisioning_job_request_for("search-index"),
        provisioning_job_request_for("localstack-bucket"),
    ];
    let engine = RecordingBatchProvisioningEngine {
        delay: StdDuration::from_millis(20),
        ..RecordingBatchProvisioningEngine::default()
    };
    let maximum_active = Arc::clone(&engine.maximum_active);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    let results = runtime.block_on(run_provisioning_jobs_from_observed(
        &engine,
        ProvisioningJobsRunOptions {
            requests: &requests,
            observed: &[],
            installation_id: "install-1",
            schema_version: 8,
            timeout: StdDuration::from_secs(30),
            concurrency: NonZeroUsize::new(2).expect("non-zero concurrency"),
        },
    ));
    let names = results
        .into_iter()
        .map(|(request, result)| {
            result.expect("run provisioning job");

            request.name().to_owned()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        requests
            .iter()
            .map(|request| request.name().to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(maximum_active.load(Ordering::SeqCst), 2);
}

#[test]
fn failed_provisioning_jobs_are_removed_and_reported() {
    let request = provisioning_job_request("install-1");
    let mut engine = RecordingSharedVolumeEngine {
        completion_fails: true,
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(run_provisioning_job(
            &mut engine,
            ProvisioningJobOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(30),
            },
        ))
        .expect_err("failed provisioning job");

    assert!(error.to_string().contains("exited with status 1"));
    assert!(matches!(
        error,
        SharedInfrastructureReconcileError::ProvisioningFailed { status_code: 1, .. }
    ));
    assert_eq!(engine.removed_containers.len(), 1);
}

#[test]
fn timed_out_provisioning_jobs_are_retained_for_recovery() {
    let request = provisioning_job_request("install-1");
    let mut engine = RecordingSharedVolumeEngine {
        completion_times_out: true,
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(run_provisioning_job(
            &mut engine,
            ProvisioningJobOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(30),
            },
        ))
        .expect_err("timed out provisioning job");

    assert!(error.to_string().contains("timed out"));
    assert!(engine.removed_containers.is_empty());
}

#[test]
fn compatible_object_stores_get_one_private_persistent_instance() {
    for (implementation, flavor, health_path) in [
        ("minio", ObjectStoreFlavor::Minio, "/minio/health/ready"),
        ("rustfs", ObjectStoreFlavor::RustFs, "/health/ready"),
    ] {
        let shared = plan_shared_instances(vec![SharedServiceRequest::new(
            "bill",
            "s3",
            object_store_profile(implementation, "1"),
        )])
        .pop()
        .expect("shared object-store plan");
        let instance = ObjectStoreSharedInstancePlan::new(
            &shared,
            ObjectStoreSharedInstancePlanOptions {
                installation_id: "install-1".to_owned(),
                network_name: "stackctl".to_owned(),
                schema_version: 8,
                desired_revision: "sha256:object-store-v1".to_owned(),
                policy_directory: "/private/object-store/policies".into(),
                root_secret: CredentialSecret::new("root-secret".to_owned()),
            },
        )
        .expect("object-store instance");

        assert_eq!(instance.flavor(), flavor);
        assert_eq!(instance.container().network(), Some("stackctl"));
        assert!(instance.container().port_bindings().is_empty());
        if flavor == ObjectStoreFlavor::Minio {
            assert_eq!(instance.container().command(), ["server", "/data"]);
        } else {
            assert!(instance.container().command().is_empty());
        }
        assert_eq!(instance.container().volume_mounts().len(), 1);
        assert_eq!(instance.data_mount_target(), "/data");
        assert_eq!(instance.container().bind_mounts().len(), 1);
        assert!(instance.container().bind_mounts()[0].is_read_only());
        assert_eq!(
            instance.policy_mount_target(),
            "/etc/stackctl/object-store/policies"
        );
        assert_eq!(
            instance
                .container()
                .health_check()
                .expect("health check")
                .engine_test(),
            vec![
                "CMD".to_owned(),
                "curl".to_owned(),
                "--fail".to_owned(),
                "--silent".to_owned(),
                format!("http://127.0.0.1:9000{health_path}"),
            ]
        );
        assert_eq!(instance.root_credential().username(), "stackctl_admin");
        assert_eq!(instance.root_credential().secret(), "root-secret");
        assert!(!format!("{instance:?}").contains("root-secret"));
    }
}

#[test]
fn gotenberg_is_one_private_stateless_instance_per_exact_profile() {
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "pdf", gotenberg_profile("8")),
        SharedServiceRequest::new("shop", "pdf", gotenberg_profile("8")),
    ])
    .pop()
    .expect("shared Gotenberg plan");
    let instance = GotenbergSharedInstancePlan::new(
        &shared,
        GotenbergSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:gotenberg-v1".to_owned(),
        },
    )
    .expect("Gotenberg instance");

    assert!(instance.container().port_bindings().is_empty());
    assert!(instance.container().volume_mounts().is_empty());
    assert_eq!(instance.container().network(), Some("stackctl"));
    assert_eq!(
        instance
            .container()
            .health_check()
            .expect("Gotenberg health")
            .engine_test(),
        [
            "CMD",
            "curl",
            "--fail",
            "--silent",
            "http://127.0.0.1:3000/health",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    );
    assert_eq!(shared.consumers().len(), 2);
}

#[test]
fn gotenberg_projects_receive_the_shared_internal_endpoint() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "pdf",
        gotenberg_profile("8"),
    )])
    .pop()
    .expect("shared Gotenberg plan");
    let instance = GotenbergSharedInstancePlan::new(
        &shared,
        GotenbergSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:gotenberg-v1".to_owned(),
        },
    )
    .expect("Gotenberg instance");

    let project = plan_gotenberg_project_resources("bill", "gotenberg", &instance)
        .expect("Gotenberg project resources");

    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([(
            "GOTENBERG_URL".to_owned(),
            format!("http://{}:3000", instance.container().name()),
        )])
    );
}

#[test]
fn object_store_projects_get_bucket_scoped_credentials_and_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "s3",
        object_store_profile("minio", "1"),
    )])
    .pop()
    .expect("shared object-store plan");
    let instance = ObjectStoreSharedInstancePlan::new(
        &shared,
        ObjectStoreSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:object-store-v1".to_owned(),
            policy_directory: "/private/object-store/policies".into(),
            root_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("object-store instance");

    let project = plan_object_store_project_resources(
        "bill",
        "s3",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("object-store project resources");

    assert_eq!(project.definition().bucket(), "stackctl-bill-s3");
    assert_eq!(project.definition().username(), "st_bill_s3");
    assert_eq!(project.definition().policy_name(), "stackctl-bill-s3");
    assert!(
        project
            .definition()
            .policy_json()
            .contains("arn:aws:s3:::stackctl-bill-s3/*")
    );
    assert!(
        project
            .definition()
            .policy_json()
            .contains("s3:GetBucketVersioning")
    );
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            ("AWS_ACCESS_KEY_ID".to_owned(), "st_bill_s3".to_owned()),
            ("AWS_BUCKET".to_owned(), "stackctl-bill-s3".to_owned()),
            ("AWS_DEFAULT_REGION".to_owned(), "us-east-1".to_owned()),
            (
                "AWS_ENDPOINT".to_owned(),
                format!("http://{}:9000", instance.container().name()),
            ),
            (
                "AWS_SECRET_ACCESS_KEY".to_owned(),
                "project-secret".to_owned()
            ),
            ("AWS_USE_PATH_STYLE_ENDPOINT".to_owned(), "true".to_owned()),
        ])
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[test]
fn minio_project_provisioning_streams_secrets_and_uses_non_shell_commands() {
    let (instance, project) = object_store_project("minio");
    let container = owned_shared_container("minio-container", "sha256:minio-1");
    let executor = RecordingObjectStoreExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_object_store_project_resources(
            &executor, &container, &instance, &project,
        ))
        .expect("provision MinIO project resources");
    runtime.block_on(tokio::task::yield_now());

    let requests = executor.requests.lock().expect("object-store requests");
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0][..3], ["mc", "mb", "--ignore-existing"]);
    assert_eq!(requests[1][..4], ["mc", "admin", "policy", "create"]);
    assert_eq!(requests[2], ["mc", "admin", "user", "add", "stackctl"]);
    assert_eq!(requests[3][..4], ["mc", "admin", "policy", "attach"]);
    assert!(
        !requests
            .iter()
            .flatten()
            .any(|value| value.contains("secret"))
    );
    drop(requests);
    let inputs = executor.inputs.lock().expect("object-store inputs");
    assert!(
        inputs
            .iter()
            .any(|input| input == b"st_bill_s3\nproject-secret\n")
    );
    let debug = executor
        .request_debug
        .lock()
        .expect("request debug")
        .join("\n");
    assert!(debug.contains("MC_HOST_stackctl"));
    assert!(!debug.contains("root-secret"));
    assert!(!debug.contains("project-secret"));
}

#[test]
fn rustfs_project_provisioning_stays_closed_until_iam_is_proven() {
    let (instance, project) = object_store_project("rustfs");
    let container = owned_shared_container("rustfs-container", "sha256:rustfs-1");
    let executor = RecordingObjectStoreExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(provision_object_store_project_resources(
            &executor, &container, &instance, &project,
        ))
        .expect_err("unproven RustFS IAM");

    assert!(
        error
            .to_string()
            .contains("RustFS IAM provisioning is not proven")
    );
    assert!(
        executor
            .requests
            .lock()
            .expect("object-store requests")
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn minio_reconciliation_persists_policy_before_instance_and_logical_resources() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-minio-reconcile-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove stale MinIO fixture");
    }
    let (instance, project) = object_store_project_at("minio", &root);
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_object_store_project_resources(
            &mut engine,
            &instance,
            &project,
            &root,
            "install-1",
            8,
        ))
        .expect("reconcile MinIO project resources");

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert_eq!(
        std::fs::read_to_string(root.join("stackctl-bill-s3.json")).expect("stored MinIO policy"),
        project.definition().policy_json()
    );
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );
    assert_eq!(
        engine
            .command_arguments
            .lock()
            .expect("MinIO commands")
            .len(),
        4
    );

    std::fs::remove_dir_all(&root).expect("remove MinIO fixture");
}

#[cfg(unix)]
#[test]
fn minio_reconciliation_rejects_policy_mount_drift_before_writes() {
    let root =
        std::env::temp_dir().join(format!("stackctl-v8-minio-reject-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove stale MinIO fixture");
    }
    let (instance, project) = object_store_project_at("minio", &root.join("wrong"));
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_object_store_project_resources(
            &mut engine,
            &instance,
            &project,
            &root,
            "install-1",
            8,
        ))
        .expect_err("policy mount drift");

    assert!(error.to_string().contains("policy mount"));
    assert!(!root.exists());
    assert!(engine.operations.is_empty());
}

#[test]
fn foreign_provisioning_jobs_fail_before_engine_mutation() {
    let request = provisioning_job_request("install-1");
    let foreign = provisioning_job_request("install-2");
    let mut engine = RecordingSharedVolumeEngine {
        observed_containers: vec![ObservedContainer::new(
            ContainerId::new("foreign-job"),
            foreign.metadata().labels(),
        )],
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(run_provisioning_job(
            &mut engine,
            ProvisioningJobOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(30),
            },
        ))
        .expect_err("foreign provisioning job");

    assert!(error.to_string().contains("owned by another installation"));
    assert!(engine.operations.is_empty());
}

#[test]
fn stale_owned_provisioning_jobs_finish_before_the_desired_job_runs() {
    let request = provisioning_job_request("install-1");
    let stale_metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::ProvisioningJob,
            project_id: Some("bill".to_owned()),
            compatibility_fingerprint: "sha256:minio-client".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:stale-bucket".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Disposable,
        },
    )
    .and_then(|metadata| metadata.with_resource_id("object-store-bucket"))
    .expect("stale provisioning metadata");
    let pass_observation = [ObservedContainer::new(
        ContainerId::new("stale-job"),
        stale_metadata.labels(),
    )];
    let mut engine = RecordingSharedVolumeEngine {
        observed_containers: Vec::new(),
        state: crate::control_plane::engine::ContainerState::Stopped,
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    runtime
        .block_on(run_provisioning_job_from_observed(
            &mut engine,
            &pass_observation,
            ProvisioningJobOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(30),
            },
        ))
        .expect("replace stale provisioning job");

    assert_eq!(engine.created_containers, vec![request]);
    assert_eq!(engine.removed_containers.len(), 2);
    assert_eq!(*engine.completions.lock().expect("completion count"), 2);
}

#[test]
fn shared_volume_reconciliation_adopts_exact_owned_data_without_replacement() {
    let request = shared_volume_request();
    let mut engine = RecordingSharedVolumeEngine {
        observed: vec![crate::control_plane::engine::ObservedVolume::new(
            request.name(),
            request.metadata().labels(),
        )],
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_shared_volume(
            &mut engine,
            SharedVolumeReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("adopt shared volume");

    assert_eq!(result.action(), SharedVolumeReconcileAction::Unchanged);
    assert!(engine.created.is_empty());
    assert!(engine.removed.is_empty());
}

#[test]
fn shared_volume_revision_changes_never_replace_compatible_data() {
    let observed_request = shared_volume_request_with_revision("sha256:desired-v1");
    let request = shared_volume_request_with_revision("sha256:desired-v2");
    let mut engine = RecordingSharedVolumeEngine {
        observed: vec![crate::control_plane::engine::ObservedVolume::new(
            request.name(),
            observed_request.metadata().labels(),
        )],
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_shared_volume(
            &mut engine,
            SharedVolumeReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("retain compatible shared volume");

    assert_eq!(result.action(), SharedVolumeReconcileAction::Unchanged);
    assert!(engine.created.is_empty());
    assert!(engine.removed.is_empty());
}

#[test]
fn shared_volume_reconciliation_refuses_foreign_name_ownership() {
    let request = shared_volume_request();
    let foreign = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-2".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::Volume,
            project_id: None,
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("foreign volume metadata");
    let mut engine = RecordingSharedVolumeEngine {
        observed: vec![crate::control_plane::engine::ObservedVolume::new(
            request.name(),
            foreign.labels(),
        )],
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_shared_volume(
            &mut engine,
            SharedVolumeReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect_err("foreign volume ownership");

    assert_eq!(
        error.to_string(),
        format!(
            "shared volume '{}' exists without current-installation ownership",
            request.name()
        )
    );
    assert!(engine.created.is_empty());
    assert!(engine.removed.is_empty());
}

#[test]
fn shared_services_stop_only_after_their_last_active_logical_reference_is_released() {
    let metadata = shared_container_metadata("install-1", "sha256:desired-v1");
    let observed =
        ObservedContainer::new(ContainerId::new("postgres-container"), metadata.labels());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "postgres-container".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "postgres-volume".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgresql_database".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:database".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let mut engine = RecordingSharedVolumeEngine {
        observed_containers: vec![observed],
        state: crate::control_plane::engine::ContainerState::Running,
        ..Default::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    let referenced = runtime
        .block_on(stop_unreferenced_shared_services(
            &mut engine,
            UnreferencedSharedServiceOptions {
                resources: std::slice::from_ref(&resource),
                logical_resources: std::slice::from_ref(&logical),
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("retain referenced shared service");
    let unreferenced = runtime
        .block_on(stop_unreferenced_shared_services(
            &mut engine,
            UnreferencedSharedServiceOptions {
                resources: std::slice::from_ref(&resource),
                logical_resources: &[],
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("stop unreferenced shared service");

    assert_eq!(referenced, 0);
    assert_eq!(unreferenced, 1);
    assert_eq!(engine.stopped_containers.len(), 1);
    assert!(engine.removed_containers.is_empty());
    assert!(engine.removed.is_empty());
}

#[test]
fn shared_service_idling_reuses_a_pass_wide_observation() {
    let metadata = shared_container_metadata("install-1", "sha256:desired-v1");
    let observed = [ObservedContainer::new(
        ContainerId::new("postgres-container"),
        metadata.labels(),
    )];
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "postgres-container".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let mut engine = RecordingSharedVolumeEngine {
        state: crate::control_plane::engine::ContainerState::Running,
        ..Default::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let stopped = runtime
        .block_on(stop_unreferenced_shared_services_from_observed(
            &mut engine,
            &observed,
            UnreferencedSharedServiceOptions {
                resources: &[resource],
                logical_resources: &[],
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("stop shared service from shared observation");

    assert_eq!(stopped, 1);
    assert_eq!(engine.stopped_containers.len(), 1);
    assert!(engine.removed_containers.is_empty());
}

#[test]
fn missing_shared_services_create_volume_before_starting_one_container() {
    let volume = shared_volume_request();
    let container = shared_container_request("sha256:desired-v1");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_shared_service(
            &mut engine,
            SharedServiceReconcileOptions {
                request: &container,
                volume: Some(&volume),
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("create shared service");

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert_eq!(result.container().metadata(), container.metadata());
    assert_eq!(
        result.volume().expect("shared volume").action(),
        SharedVolumeReconcileAction::Created
    );
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );
}

#[test]
fn shared_service_revision_drift_replaces_only_the_container() {
    let old_volume = shared_volume_request_with_revision("sha256:desired-v1");
    let volume = shared_volume_request_with_revision("sha256:desired-v2");
    let old_container = shared_container_request("sha256:desired-v1");
    let container = shared_container_request("sha256:desired-v2");
    let mut engine = RecordingSharedVolumeEngine {
        observed: vec![crate::control_plane::engine::ObservedVolume::new(
            volume.name(),
            old_volume.metadata().labels(),
        )],
        observed_containers: vec![ObservedContainer::new(
            ContainerId::new("postgres-17"),
            old_container.metadata().labels(),
        )],
        state: crate::control_plane::engine::ContainerState::Running,
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_shared_service(
            &mut engine,
            SharedServiceReconcileOptions {
                request: &container,
                volume: Some(&volume),
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("replace shared service container");

    assert_eq!(result.action(), SharedServiceReconcileAction::Replaced);
    assert_eq!(
        engine.operations,
        vec![
            "stop-container",
            "remove-container",
            "create-container",
            "start-container"
        ]
    );
    assert!(engine.created.is_empty());
    assert!(engine.removed.is_empty());
}

#[test]
fn foreign_shared_service_conflicts_before_volume_creation() {
    let volume = shared_volume_request();
    let container = shared_container_request("sha256:desired-v1");
    let foreign = shared_container_metadata("install-2", "sha256:desired-v1");
    let mut engine = RecordingSharedVolumeEngine {
        observed_containers: vec![ObservedContainer::new(
            ContainerId::new("foreign-postgres-17"),
            foreign.labels(),
        )],
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_shared_service(
            &mut engine,
            SharedServiceReconcileOptions {
                request: &container,
                volume: Some(&volume),
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect_err("foreign shared service");

    assert_eq!(
        error.to_string(),
        "shared service compatibility 'sha256:postgres-17' is already owned by another installation"
    );
    assert!(engine.operations.is_empty());
}

#[test]
fn stale_shared_service_discovery_recreates_the_missing_container() {
    let container = shared_container_request("sha256:desired-v1");
    let mut engine = RecordingSharedVolumeEngine {
        observed_containers: vec![ObservedContainer::new(
            ContainerId::new("stale-postgres-17"),
            container.metadata().labels(),
        )],
        state: crate::control_plane::engine::ContainerState::Missing,
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_shared_service(
            &mut engine,
            SharedServiceReconcileOptions {
                request: &container,
                volume: None,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("recover missing shared service");

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert_eq!(
        engine.operations,
        vec!["create-container", "start-container"]
    );
}

#[test]
fn different_major_versions_never_share_a_fingerprint() {
    let postgres_17 = fingerprint(vec!["postgis"], "17");
    let postgres_18 = fingerprint(vec!["postgis"], "18");

    assert_ne!(postgres_17, postgres_18);
}

#[test]
fn immutable_settings_participate_in_the_fingerprint() {
    let mut options = postgres_options(vec!["postgis"], "17");
    let baseline =
        CompatibilityFingerprint::from_options(options.clone()).expect("baseline fingerprint");
    options
        .immutable_settings
        .insert("locale".to_owned(), "fi_FI.UTF-8".to_owned());

    let localized = CompatibilityFingerprint::from_options(options).expect("localized fingerprint");

    assert_ne!(baseline, localized);
}

#[test]
fn incomplete_compatibility_profiles_fail_before_planning() {
    let mut options = postgres_options(Vec::new(), "17");
    options.implementation.clear();

    let error = CompatibilityFingerprint::from_options(options)
        .expect_err("missing implementation identity");

    assert_eq!(
        error.to_string(),
        "compatibility implementation must not be empty"
    );
}

#[test]
fn mutable_image_references_cannot_identify_shared_instances() {
    let mut options = postgres_options(Vec::new(), "17");
    options.image_digest = "postgres:17".to_owned();

    let error =
        CompatibilityFingerprint::from_options(options).expect_err("mutable image reference");

    assert_eq!(
        error.to_string(),
        "compatibility image 'postgres:17' must use an immutable sha256 digest"
    );
}

#[test]
fn forty_projects_across_two_postgres_majors_plan_two_instances() {
    let requests = (0..40)
        .map(|index| {
            let major = if index < 20 { "17" } else { "18" };

            SharedServiceRequest::new(
                format!("project-{index:02}"),
                "database",
                profile(vec!["postgis"], major),
            )
        })
        .collect();

    let plans = plan_shared_instances(requests);

    assert_eq!(plans.len(), 2);
    assert_eq!(
        plans
            .iter()
            .map(|plan| plan.consumers().len())
            .collect::<Vec<_>>(),
        vec![20, 20]
    );
    assert_eq!(
        plans
            .iter()
            .map(|plan| plan.fingerprint())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    assert_eq!(plans[0].profile().implementation(), "postgresql");
    assert!(plans[0].profile().image_digest().contains("@sha256:"));
}

#[test]
fn repeated_identical_consumers_do_not_duplicate_logical_ownership() {
    let request = SharedServiceRequest::new("bill", "database", profile(Vec::new(), "17"));

    let plans = plan_shared_instances(vec![request.clone(), request]);
    let consumers = plans
        .first()
        .map(|plan| plan.consumers())
        .unwrap_or_default();

    assert_eq!(plans.len(), 1);
    assert_eq!(consumers.len(), 1);
    assert_eq!(
        consumers.first().map(|owner| owner.project_id()),
        Some("bill")
    );
    assert_eq!(
        consumers.first().map(|owner| owner.service_id()),
        Some("database")
    );
}

#[test]
fn managed_credentials_use_256_bits_of_injected_entropy_and_redact_debug() {
    let secret = generate_credential_secret(&SequentialEntropy).expect("managed secret");

    assert_eq!(
        secret.expose(),
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
    );
    assert_eq!(format!("{secret:?}"), "CredentialSecret([REDACTED])");
}

#[test]
fn mongodb_shared_instances_use_redacted_environment_bootstrap_and_retained_data() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        mongodb_profile("8"),
    )])
    .pop()
    .expect("shared MongoDB plan");
    let plan = MongoDbSharedInstancePlan::new(
        &shared,
        MongoDbSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mongodb-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mongo-root".to_owned()),
        },
    )
    .expect("MongoDB instance");

    assert_eq!(plan.data_mount_target(), "/data/db");
    assert!(plan.volume().is_some());
    assert_eq!(plan.bootstrap_credential().username(), "stackctl_admin");
    assert!(plan.container().bind_mounts().is_empty());
    assert_eq!(
        plan.container()
            .environment()
            .get("MONGO_INITDB_ROOT_PASSWORD")
            .map(String::as_str),
        Some("mongo-root")
    );
    assert!(
        !plan
            .container()
            .environment()
            .contains_key("MONGO_INITDB_ROOT_PASSWORD_FILE")
    );
    let debug = format!("{:?}", plan.container());
    assert!(debug.contains("MONGO_INITDB_ROOT_PASSWORD"));
    assert!(!debug.contains("mongo-root"));
}

#[cfg(unix)]
#[test]
fn managed_secret_store_is_private_immutable_and_exact() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-managed-secret-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    drop(std::fs::remove_dir_all(&root));
    let path = root.join("mongodb-root");
    let secret = CredentialSecret::new("root-secret".to_owned());

    let stored = store_credential_secret(&secret, &path).expect("store secret");
    let interrupted = root.join(".mongodb-root.tmp");
    std::fs::write(&interrupted, "interrupted-secret").expect("interrupted secret write");
    store_credential_secret(&secret, &path).expect("reconcile secret");
    let error =
        store_credential_secret(&CredentialSecret::new("different-secret".to_owned()), &path)
            .expect_err("reject secret replacement");

    assert_eq!(stored, path);
    assert!(!interrupted.exists());
    assert_eq!(
        std::fs::read_to_string(&path).expect("secret file"),
        "root-secret"
    );
    assert_eq!(
        std::fs::metadata(&root)
            .expect("secret directory")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&path)
            .expect("secret file")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(error.to_string().contains("refusing to replace"));
    assert!(!error.to_string().contains("root-secret"));
    assert!(!error.to_string().contains("different-secret"));

    std::fs::remove_dir_all(&root).expect("remove managed-secret fixture");
}

#[cfg(unix)]
#[test]
fn managed_secret_store_refuses_symbolic_link_targets_without_mutation() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "stackctl-managed-secret-symlink-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    drop(std::fs::remove_dir_all(&root));
    std::fs::create_dir_all(&root).expect("create managed-secret fixture");
    let victim = root.join("victim");
    std::fs::write(&victim, "root-secret").expect("write victim");
    std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o640))
        .expect("set victim permissions");
    let path = root.join("mongodb-root");
    symlink(&victim, &path).expect("create managed-secret symlink");

    let error = store_credential_secret(&CredentialSecret::new("root-secret".to_owned()), &path)
        .expect_err("symbolic-link managed secret must fail closed");

    assert!(error.to_string().contains("symbolic link"));
    assert_eq!(
        std::fs::read_to_string(&victim).expect("read victim"),
        "root-secret"
    );
    assert_eq!(
        std::fs::metadata(&victim)
            .expect("victim metadata")
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert!(
        std::fs::symlink_metadata(&path)
            .expect("managed-secret symlink")
            .file_type()
            .is_symlink()
    );

    std::fs::remove_dir_all(&root).expect("remove managed-secret fixture");
}

#[test]
fn mongodb_logical_resources_are_idempotent_database_scoped_and_stdin_only() {
    let plan = MongoDbLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
        CredentialSecret::new("mongo-root".to_owned()),
    )
    .expect("MongoDB logical plan");

    assert_eq!(plan.database_name(), "stackctl_bill_database");
    assert_eq!(plan.username(), "st_bill_database");
    assert_eq!(plan.credential_id(), "bill/database/mongodb");
    assert_eq!(plan.command_arguments(), ["mongosh", "--quiet", "--nodb"]);
    assert!(plan.stdin_script().contains("getUser"));
    assert!(plan.stdin_script().contains("updateUser"));
    assert!(plan.stdin_script().contains("createUser"));
    assert!(plan.stdin_script().contains("readWrite"));
    assert!(plan.stdin_script().contains("project-secret"));
    assert!(plan.stdin_script().contains("mongo-root"));
    assert!(!format!("{plan:?}").contains("project-secret"));
    assert!(!format!("{plan:?}").contains("mongo-root"));
}

#[test]
fn mongodb_project_resources_compose_user_credential_and_environment() {
    let (instance, _) = mongodb_instance();
    let project = plan_mongodb_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MongoDB project resources");

    assert_eq!(project.logical().database_name(), "stackctl_bill_database");
    assert_eq!(project.credential().username(), "st_bill_database");
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            (
                "MONGODB_DATABASE".to_owned(),
                "stackctl_bill_database".to_owned()
            ),
            (
                "MONGODB_HOST".to_owned(),
                instance.container().name().to_owned()
            ),
            ("MONGODB_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("MONGODB_PORT".to_owned(), "27017".to_owned()),
            ("MONGODB_USERNAME".to_owned(), "st_bill_database".to_owned()),
        ])
    );
    assert!(!format!("{project:?}").contains("project-secret"));
    assert!(!format!("{project:?}").contains("mongo-root"));
}

#[test]
fn mongodb_shared_reconciliation_records_the_physical_database_identity() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-mongodb-logical-identity-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create MongoDB logical identity state");
    let mut store =
        SqliteStateStore::open(&root.join("state.sqlite3")).expect("MongoDB state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mongodb", "8"),
    )]);
    let prepared = prepare_mongodb_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        super::MongoDbPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
        },
    )
    .expect("prepare MongoDB instance");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_mongodb_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("reconcile prepared MongoDB instance");

    assert_eq!(result.logical_resources().len(), 1);
    assert_eq!(
        result.logical_resources()[0].logical_resource_id(),
        "stackctl_bill_database",
        "backup and restore must address the physical MongoDB database"
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove MongoDB logical identity state");
}

#[test]
fn mongodb_migration_target_is_separate_owned_retained_and_environment_bootstrapped() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mongodb", "8"),
    )])
    .pop()
    .expect("shared MongoDB plan");
    let target = MongoDbSharedInstancePlan::new_migration_target(
        &shared,
        MongoDbMigrationInstancePlanOptions {
            migration_id: "restore-42".to_owned(),
            project_id: "bill".to_owned(),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mongodb-target".to_owned(),
            bootstrap_secret: CredentialSecret::new("target-root".to_owned()),
        },
    )
    .expect("MongoDB migration target");

    assert_eq!(target.container().name(), "stackctl-migration-restore-42");
    assert_eq!(target.container().metadata().project_id(), Some("bill"));
    assert_eq!(
        target.container().metadata().resource_id(),
        Some("restore-42")
    );
    assert_eq!(
        target.container().metadata().retention(),
        crate::control_plane::engine::RetentionClass::Persistent
    );
    assert_eq!(
        target.bootstrap_credential().credential_id(),
        "migration/restore-42/mongodb-bootstrap"
    );
    assert!(target.volume().is_some());
    assert!(target.container().bind_mounts().is_empty());
    assert_eq!(
        target
            .container()
            .environment()
            .get("MONGO_INITDB_ROOT_PASSWORD")
            .map(String::as_str),
        Some("target-root")
    );
    let debug = format!("{:?}", target.container());
    assert!(debug.contains("MONGO_INITDB_ROOT_PASSWORD"));
    assert!(!debug.contains("target-root"));
}

#[test]
fn mongodb_migration_target_reuses_durable_credential_and_secret_path() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-mongodb-migration-target-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let database_path = root.join("state.sqlite3");
    std::fs::create_dir_all(&root).expect("create MongoDB target state directory");
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mongodb", "8"),
    )])
    .pop()
    .expect("shared MongoDB plan");
    let options = MongoDbMigrationPreparationOptions {
        migration_id: "restore-42",
        project_id: "bill",
        installation_id: "install-1",
        network_name: "stackctl",
        schema_version: 8,
        desired_revision: "sha256:mongodb-target",
    };

    let first = prepare_mongodb_migration_target(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        options,
    )
    .expect("first MongoDB target preparation");
    let second = prepare_mongodb_migration_target(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        options,
    )
    .expect("replayed MongoDB target preparation");

    assert_eq!(first.container(), second.container());
    assert_eq!(first.volume(), second.volume());
    assert_eq!(first.bootstrap_credential(), second.bootstrap_credential());
    assert_eq!(store.credentials().expect("credentials").len(), 1);

    drop(store);
    std::fs::remove_dir_all(root).expect("remove MongoDB target state");
}

#[test]
fn mongodb_migration_target_reconciliation_converges_retained_service() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-mongodb-migration-reconcile-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create MongoDB reconciliation directory");
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mongodb", "8"),
    )])
    .pop()
    .expect("shared MongoDB plan");
    let mut engine = RecordingSharedVolumeEngine {
        health: crate::control_plane::engine::ContainerHealth::RunningUnverified,
        command_exits: Arc::new(Mutex::new(VecDeque::from([1, 0]))),
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_mongodb_migration_target(
            &mut store,
            &mut engine,
            &shared,
            &FixedCredentialEntropy(0x11),
            MongoDbMigrationPreparationOptions {
                migration_id: "restore-42",
                project_id: "bill",
                installation_id: "install-1",
                network_name: "stackctl",
                schema_version: 8,
                desired_revision: "sha256:mongodb-target",
            },
        ))
        .expect("MongoDB migration target reconciliation");

    assert_eq!(
        result.container().metadata().resource_id(),
        Some("restore-42")
    );
    assert_eq!(result.volume().name(), "stackctl-migration-restore-42-data");
    assert_eq!(
        result
            .plan()
            .container()
            .environment()
            .get("MONGO_INITDB_ROOT_PASSWORD")
            .map(String::as_str),
        Some(result.bootstrap_credential().secret())
    );
    assert_eq!(
        result.health(),
        crate::control_plane::engine::ContainerHealth::Healthy
    );
    assert_eq!(
        engine
            .command_arguments
            .lock()
            .expect("MongoDB readiness commands")
            .len(),
        2,
        "migration target readiness must retry an authenticated protocol probe"
    );
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove MongoDB reconciliation state");
}

#[test]
fn mongodb_project_reconciliation_converges_instance_and_database_user() {
    let (instance, _) = mongodb_instance();
    let project = plan_mongodb_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MongoDB project resources");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_mongodb_project_resources(
            &mut engine,
            &instance,
            &project,
            "install-1",
            8,
        ))
        .expect("reconcile MongoDB project resources");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    let input = String::from_utf8(
        engine
            .command_input
            .lock()
            .expect("MongoDB command input")
            .clone(),
    )
    .expect("MongoDB script UTF-8");
    assert!(input.contains("project-secret"));
    assert!(input.contains("mongo-root"));
}

#[test]
fn mongodb_provisioning_streams_both_secrets_and_checks_exit_status() {
    let (instance, container) = mongodb_instance();
    let project = plan_mongodb_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MongoDB project resources");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_mongodb_logical_resource(
            &executor,
            &container,
            project.logical(),
        ))
        .expect("provision MongoDB resource");
    runtime.block_on(tokio::task::yield_now());

    let stdin = String::from_utf8(executor.stdin.lock().expect("recorded stdin").clone())
        .expect("script UTF-8");
    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(stdin.contains("project-secret"));
    assert!(stdin.contains("mongo-root"));
    assert!(request_debug.contains("argument_count: 3"));
    assert!(!request_debug.contains("project-secret"));
    assert!(!request_debug.contains("mongo-root"));
}

#[test]
fn mongodb_readiness_uses_stdin_auth_without_exposing_the_bootstrap_secret() {
    let (_, container) = mongodb_instance();
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/mongodb/bootstrap".to_owned(),
        project_id: None,
        service_id: "mongodb".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "mongo-root".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(wait_for_mongodb_readiness(
            &executor,
            &container,
            &administrator,
        ))
        .expect("wait for MongoDB readiness");
    runtime.block_on(tokio::task::yield_now());

    let stdin = String::from_utf8(executor.stdin.lock().expect("recorded stdin").clone())
        .expect("MongoDB readiness script UTF-8");
    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(stdin.contains("admin.auth"));
    assert!(stdin.contains("ping"));
    assert!(stdin.contains("mongo-root"));
    assert!(request_debug.contains("argument_count: 3"));
    assert!(!request_debug.contains("mongo-root"));
}

#[test]
fn rabbitmq_password_hashes_match_the_documented_salted_sha256_format() {
    let hash = RabbitMqPasswordHash::from_salt(
        CredentialSecret::new("project-secret".to_owned()),
        [1, 2, 3, 4],
    );

    assert_eq!(
        hash.encoded(),
        "AQIDBC+1G1c0sCkZnSk4UmOklfTffD0p+fnkx/QqloCOZ0es"
    );
    assert_eq!(format!("{hash:?}"), "RabbitMqPasswordHash([REDACTED])");
}

#[test]
fn rabbitmq_definitions_are_deterministic_isolated_and_hash_only() {
    let definitions = RabbitMqDefinitions::new(vec![
        RabbitMqProjectDefinition::new(
            "shop",
            "broker",
            CredentialSecret::new("shop-secret".to_owned()),
        )
        .expect("shop definition"),
        RabbitMqProjectDefinition::new(
            "bill",
            "broker",
            CredentialSecret::new("bill-secret".to_owned()),
        )
        .expect("bill definition"),
    ])
    .expect("RabbitMQ definitions");
    let document: serde_json::Value =
        serde_json::from_slice(definitions.contents()).expect("definitions JSON");

    assert_eq!(
        document["vhosts"],
        serde_json::json!([
            {"name": "stackctl_bill_broker"},
            {"name": "stackctl_shop_broker"}
        ])
    );
    assert_eq!(document["users"][0]["name"], "st_bill_broker");
    assert_eq!(
        document["users"][0]["hashing_algorithm"],
        "rabbit_password_hashing_sha256"
    );
    assert_eq!(
        document["users"][0]["tags"],
        serde_json::json!(["management"])
    );
    assert_eq!(
        document["permissions"][0],
        serde_json::json!({
            "user": "st_bill_broker",
            "vhost": "stackctl_bill_broker",
            "configure": ".*",
            "write": ".*",
            "read": ".*"
        })
    );
    assert!(!String::from_utf8_lossy(definitions.contents()).contains("secret"));
    assert_eq!(
        format!("{definitions:?}"),
        "RabbitMqDefinitions { project_count: 2 }"
    );
}

#[test]
fn rabbitmq_definitions_reject_deterministic_identity_collisions() {
    let definitions = vec![
        RabbitMqProjectDefinition::new(
            "bill-api",
            "broker",
            CredentialSecret::new("one".to_owned()),
        )
        .expect("first definition"),
        RabbitMqProjectDefinition::new(
            "bill",
            "api-broker",
            CredentialSecret::new("two".to_owned()),
        )
        .expect("second definition"),
    ];

    let error = RabbitMqDefinitions::new(definitions).expect_err("identity collision");

    assert_eq!(
        error.to_string(),
        "RabbitMQ user 'st_bill_api_broker' is defined more than once"
    );
}

#[cfg(unix)]
#[test]
fn rabbitmq_definitions_store_is_private_hash_only_and_atomically_replaceable() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-rabbitmq-definitions-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    drop(std::fs::remove_dir_all(&root));
    let initial = RabbitMqDefinitions::new(Vec::new()).expect("initial definitions");
    let replacement = RabbitMqDefinitions::new(vec![
        RabbitMqProjectDefinition::new(
            "bill",
            "broker",
            CredentialSecret::new("project-secret".to_owned()),
        )
        .expect("project definition"),
    ])
    .expect("replacement definitions");

    let stored = store_rabbitmq_definitions(&initial, &root).expect("initial store");
    let interrupted = root.join("mounted/.definitions.json.tmp");
    std::fs::write(&interrupted, "partial definitions").expect("interrupted definitions write");
    store_rabbitmq_definitions(&replacement, &root).expect("replacement store");

    assert_eq!(stored.directory(), root);
    assert!(!interrupted.exists());
    assert_eq!(stored.mount_directory(), root.join("mounted"));
    assert_eq!(stored.config_file(), root.join("mounted/rabbitmq.conf"));
    assert_eq!(
        stored.definitions_file(),
        root.join("mounted/definitions.json")
    );
    assert_eq!(
        std::fs::read_to_string(stored.config_file()).expect("RabbitMQ config"),
        "definitions.import_backend = local_filesystem\n\
         definitions.local.path = /etc/stackctl/rabbitmq/definitions.json\n\
         definitions.skip_if_unchanged = true\n"
    );
    let contents =
        std::fs::read_to_string(stored.definitions_file()).expect("RabbitMQ definitions");
    assert_eq!(contents.as_bytes(), replacement.contents());
    assert!(!contents.contains("project-secret"));
    assert_eq!(
        std::fs::metadata(stored.directory())
            .expect("private root")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(stored.mount_directory())
            .expect("mount directory")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::metadata(stored.definitions_file())
            .expect("definitions file")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );

    std::fs::remove_dir_all(&root).expect("remove RabbitMQ fixture");
}

#[cfg(unix)]
#[test]
fn rabbitmq_definitions_store_refuses_a_linked_immutable_config() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "stackctl-rabbitmq-config-symlink-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    drop(std::fs::remove_dir_all(&root));
    let mounted = root.join("mounted");
    std::fs::create_dir_all(&mounted).expect("create RabbitMQ mount directory");
    let victim = root.join("victim.conf");
    std::fs::write(
        &victim,
        "definitions.import_backend = local_filesystem\n\
         definitions.local.path = /etc/stackctl/rabbitmq/definitions.json\n\
         definitions.skip_if_unchanged = true\n",
    )
    .expect("write victim config");
    symlink(&victim, mounted.join("rabbitmq.conf")).expect("create config symlink");
    let definitions = RabbitMqDefinitions::new(Vec::new()).expect("empty definitions");

    let error = store_rabbitmq_definitions(&definitions, &root)
        .expect_err("linked RabbitMQ config must fail closed");

    assert!(error.to_string().contains("symbolic link"));
    assert!(
        std::fs::symlink_metadata(mounted.join("rabbitmq.conf"))
            .expect("linked RabbitMQ config")
            .file_type()
            .is_symlink()
    );

    std::fs::remove_dir_all(&root).expect("remove RabbitMQ fixture");
}

#[test]
fn rabbitmq_materializes_one_private_persistent_definition_backed_instance() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "broker",
        rabbitmq_profile("4"),
    )])
    .pop()
    .expect("shared RabbitMQ plan");
    let plan = RabbitMqSharedInstancePlan::new(
        &shared,
        RabbitMqSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:rabbitmq-v1".to_owned(),
            definitions_directory: "/private/rabbitmq/mounted".into(),
        },
    )
    .expect("RabbitMQ instance");

    assert!(plan.container().name().starts_with("stackctl-shared-"));
    assert!(plan.container().name().len() <= 63);
    assert_eq!(
        plan.container().metadata().resource_id(),
        Some(plan.container().name())
    );
    assert_eq!(plan.container().image(), shared.profile().image_digest());
    assert_eq!(plan.config_mount_target(), "/etc/stackctl/rabbitmq");
    assert_eq!(plan.data_mount_target(), "/var/lib/rabbitmq");
    assert_eq!(plan.config_file(), "/etc/stackctl/rabbitmq/rabbitmq.conf");
    assert_eq!(plan.node_name(), "rabbit@localhost");
    assert!(plan.volume().is_some());
    let debug = format!("{:?}", plan.container());
    assert!(debug.contains("/private/rabbitmq/mounted"));
    assert!(debug.contains("read_only: true"));
    assert!(debug.contains("RABBITMQ_CONFIG_FILE"));
    assert!(debug.contains("RABBITMQ_NODENAME"));
}

#[cfg(unix)]
#[test]
fn rabbitmq_reconciliation_publishes_definitions_before_start_and_reload() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-rabbitmq-reconcile-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove stale RabbitMQ fixture");
    }
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "broker",
        rabbitmq_profile("4"),
    )])
    .pop()
    .expect("shared RabbitMQ plan");
    let instance = RabbitMqSharedInstancePlan::new(
        &shared,
        RabbitMqSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:rabbitmq-v1".to_owned(),
            definitions_directory: root.join("mounted"),
        },
    )
    .expect("RabbitMQ instance");
    let definitions = RabbitMqDefinitions::new(vec![
        RabbitMqProjectDefinition::new(
            "bill",
            "broker",
            CredentialSecret::new("project-secret".to_owned()),
        )
        .expect("RabbitMQ project definition"),
    ])
    .expect("RabbitMQ definitions");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_rabbitmq_definitions(
            &mut engine,
            &instance,
            &definitions,
            &root,
            "install-1",
            8,
        ))
        .expect("reconcile RabbitMQ definitions");

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert!(root.join("mounted/definitions.json").is_file());
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );
    assert_eq!(
        *engine
            .command_arguments
            .lock()
            .expect("RabbitMQ command arguments"),
        vec![
            vec![
                "rabbitmq-diagnostics".to_owned(),
                "-q".to_owned(),
                "check_running".to_owned(),
            ],
            vec![
                "rabbitmqctl".to_owned(),
                "import_definitions".to_owned(),
                "/etc/stackctl/rabbitmq/definitions.json".to_owned(),
            ],
        ]
    );

    std::fs::remove_dir_all(&root).expect("remove RabbitMQ fixture");
}

#[cfg(unix)]
#[test]
fn rabbitmq_reconciliation_rejects_unmanaged_mounts_before_writes() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-rabbitmq-mount-reject-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove stale RabbitMQ fixture");
    }
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "broker",
        rabbitmq_profile("4"),
    )])
    .pop()
    .expect("shared RabbitMQ plan");
    let instance = RabbitMqSharedInstancePlan::new(
        &shared,
        RabbitMqSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:rabbitmq-v1".to_owned(),
            definitions_directory: root.join("wrong"),
        },
    )
    .expect("RabbitMQ instance");
    let definitions = RabbitMqDefinitions::new(Vec::new()).expect("RabbitMQ definitions");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_rabbitmq_definitions(
            &mut engine,
            &instance,
            &definitions,
            &root,
            "install-1",
            8,
        ))
        .expect_err("unmanaged definitions mount");

    assert!(
        error
            .to_string()
            .contains("definitions mount must use managed directory")
    );
    assert!(!root.exists());
    assert!(engine.operations.is_empty());
}

#[test]
fn rabbitmq_project_resources_compose_vhost_credential_and_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "broker",
        rabbitmq_profile("4"),
    )])
    .pop()
    .expect("shared RabbitMQ plan");
    let instance = RabbitMqSharedInstancePlan::new(
        &shared,
        RabbitMqSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:rabbitmq-v1".to_owned(),
            definitions_directory: "/private/rabbitmq/mounted".into(),
        },
    )
    .expect("RabbitMQ instance");

    let project = plan_rabbitmq_project_resources(
        "bill",
        "broker",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("RabbitMQ project resources");

    assert_eq!(project.definition().username(), "st_bill_broker");
    assert_eq!(project.definition().vhost(), "stackctl_bill_broker");
    assert_eq!(project.credential().credential_id(), "bill/broker/rabbitmq");
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            (
                "RABBITMQ_HOST".to_owned(),
                instance.container().name().to_owned()
            ),
            ("RABBITMQ_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("RABBITMQ_PORT".to_owned(), "5672".to_owned()),
            ("RABBITMQ_USERNAME".to_owned(), "st_bill_broker".to_owned()),
            (
                "RABBITMQ_VHOST".to_owned(),
                "stackctl_bill_broker".to_owned()
            ),
        ])
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[test]
fn rabbitmq_live_definitions_import_is_bounded_and_credential_free() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "broker",
        rabbitmq_profile("4"),
    )])
    .pop()
    .expect("shared RabbitMQ plan");
    let instance = RabbitMqSharedInstancePlan::new(
        &shared,
        RabbitMqSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:rabbitmq-v1".to_owned(),
            definitions_directory: "/private/rabbitmq/mounted".into(),
        },
    )
    .expect("RabbitMQ instance");
    let container = owned_shared_container("rabbitmq-container", "sha256:rabbitmq-4");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(reload_rabbitmq_definitions(
            &executor, &container, &instance,
        ))
        .expect("reload RabbitMQ definitions");
    runtime.block_on(tokio::task::yield_now());

    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(request_debug.contains("argument_count: 3"));
    assert!(request_debug.contains("environment_keys: []"));
    assert!(executor.stdin.lock().expect("recorded stdin").is_empty());
}

#[test]
fn rabbitmq_project_removal_revokes_existing_users_without_deleting_vhost_data() {
    let definition = RabbitMqProjectDefinition::new(
        "bill",
        "broker",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("RabbitMQ definition");
    let container = owned_shared_container("rabbitmq-container", "sha256:rabbitmq-4");
    let executor =
        RecordingOutputExecutor::new(vec![b"st_bill_broker\t[management]\n".to_vec(), Vec::new()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(revoke_rabbitmq_project_access(
            &executor,
            &container,
            &definition,
        ))
        .expect("revoke RabbitMQ access");

    assert_eq!(
        *executor.requests.lock().expect("recorded requests"),
        vec![
            vec![
                "rabbitmqctl".to_owned(),
                "list_users".to_owned(),
                "--no-table-headers".to_owned(),
            ],
            vec![
                "rabbitmqctl".to_owned(),
                "delete_user".to_owned(),
                "st_bill_broker".to_owned(),
            ],
        ]
    );
    assert!(
        executor
            .requests
            .lock()
            .expect("recorded requests")
            .iter()
            .flatten()
            .all(|argument| argument != "stackctl_bill_broker")
    );
}

#[test]
fn rabbitmq_project_access_revocation_is_idempotent_when_user_is_absent() {
    let definition = RabbitMqProjectDefinition::new(
        "bill",
        "broker",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("RabbitMQ definition");
    let container = owned_shared_container("rabbitmq-container", "sha256:rabbitmq-4");
    let executor = RecordingOutputExecutor::new(vec![b"guest\n".to_vec()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(revoke_rabbitmq_project_access(
            &executor,
            &container,
            &definition,
        ))
        .expect("already revoked RabbitMQ access");

    assert_eq!(
        executor.requests.lock().expect("recorded requests").len(),
        1
    );
}

#[test]
fn rabbitmq_project_access_revocation_bounds_broker_output() {
    let definition = RabbitMqProjectDefinition::new(
        "bill",
        "broker",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("RabbitMQ definition");
    let container = owned_shared_container("rabbitmq-container", "sha256:rabbitmq-4");
    let executor = RecordingOutputExecutor::new(vec![vec![b'x'; 1024 * 1024 + 1]]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(revoke_rabbitmq_project_access(
            &executor,
            &container,
            &definition,
        ))
        .expect_err("oversized RabbitMQ output");

    assert_eq!(
        error.to_string(),
        "list RabbitMQ users before access revocation output exceeds 1048576 bytes"
    );
}

#[test]
fn orphaned_rabbitmq_credentials_are_revoked_before_the_shared_service_idles() {
    let owned = owned_shared_container("rabbitmq-container", "sha256:rabbitmq-4");
    let observed = ObservedContainer::new(owned.id().clone(), owned.metadata().labels());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: owned.id().as_str().to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: "sha256:rabbitmq-4".to_owned(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/broker/rabbitmq".to_owned(),
        shared_resource_id: "rabbitmq-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "broker".to_owned(),
        kind: "rabbitmq_vhost_user".to_owned(),
        compatibility_fingerprint: "sha256:rabbitmq-4".to_owned(),
        desired_revision: "sha256:logical-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.logical_resource_id().to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "broker".to_owned(),
        username: "st_bill_broker".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let pass_observation = [observed];
    let mut engine = RecordingOrphanAccessEngine {
        observed: Vec::new(),
        state: crate::control_plane::engine::ContainerState::Stopped,
        started: Vec::new(),
        commands: RecordingOutputExecutor::new(vec![
            b"st_bill_broker\t[management]\n".to_vec(),
            Vec::new(),
        ]),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let revoked = runtime
        .block_on(revoke_orphaned_shared_access_from_observed(
            &mut engine,
            &pass_observation,
            OrphanedSharedAccessOptions {
                resources: &[resource],
                logical_resources: &[logical],
                credentials: &[credential],
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(15),
            },
        ))
        .expect("revoke orphaned RabbitMQ access");

    assert_eq!(revoked, 1);
    assert_eq!(engine.started, [owned]);
    assert_eq!(
        *engine.commands.requests.lock().expect("recorded requests"),
        vec![
            vec![
                "rabbitmqctl".to_owned(),
                "list_users".to_owned(),
                "--no-table-headers".to_owned(),
            ],
            vec![
                "rabbitmqctl".to_owned(),
                "delete_user".to_owned(),
                "st_bill_broker".to_owned(),
            ],
        ]
    );
}

#[test]
fn orphaned_redis_credentials_are_revoked_without_deleting_prefixed_data() {
    let fingerprint = format!("sha256:{}", "a".repeat(64));
    let owned = owned_shared_container("redis-container", &fingerprint);
    let observed = ObservedContainer::new(owned.id().clone(), owned.metadata().labels());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: owned.id().as_str().to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/cache/redis".to_owned(),
        shared_resource_id: "redis-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        kind: "redis_acl_prefix".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:logical-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.logical_resource_id().to_owned(),
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
    let mut engine = RecordingOrphanAccessEngine {
        observed: vec![observed],
        state: crate::control_plane::engine::ContainerState::Running,
        started: Vec::new(),
        commands: RecordingOutputExecutor::new(vec![b"1\n".to_vec()]),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let revoked = runtime
        .block_on(revoke_orphaned_shared_access(
            &mut engine,
            OrphanedSharedAccessOptions {
                resources: &[resource],
                logical_resources: &[logical],
                credentials: &[credential, administrator],
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(15),
            },
        ))
        .expect("revoke orphaned Redis access");

    assert_eq!(revoked, 1);
    assert_eq!(
        *engine.commands.requests.lock().expect("recorded requests"),
        [vec![
            "redis-cli".to_owned(),
            "--raw".to_owned(),
            "--user".to_owned(),
            "stackctl_admin".to_owned(),
            "ACL".to_owned(),
            "DELUSER".to_owned(),
            "st_bill_cache".to_owned(),
        ]]
    );
    assert!(
        engine.commands.requests.lock().expect("requests")[0]
            .iter()
            .all(|argument| argument != "UNLINK" && argument != "EVAL")
    );
}

#[test]
fn orphaned_postgres_roles_receive_nologin_without_dropping_the_database() {
    let fingerprint = format!("sha256:{}", "b".repeat(64));
    let owned = owned_shared_container("postgres-container", &fingerprint);
    let observed = ObservedContainer::new(owned.id().clone(), owned.metadata().labels());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: owned.id().as_str().to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_db".to_owned(),
        shared_resource_id: "postgres-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "db".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:logical-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/db/postgresql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "db".to_owned(),
        username: "stackctl_bill_db_role".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/postgresql-bootstrap", "b".repeat(64)),
        project_id: None,
        service_id: "postgresql".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let mut engine = RecordingOrphanAccessEngine {
        observed: vec![observed],
        state: crate::control_plane::engine::ContainerState::Running,
        started: Vec::new(),
        commands: RecordingOutputExecutor::new(vec![b"true\n".to_vec()]),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let revoked = runtime
        .block_on(revoke_orphaned_shared_access(
            &mut engine,
            OrphanedSharedAccessOptions {
                resources: &[resource],
                logical_resources: &[logical],
                credentials: &[credential, administrator],
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(15),
            },
        ))
        .expect("revoke orphaned PostgreSQL access");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(revoked, 1);
    let stdin = engine.commands.stdin.lock().expect("recorded stdin");
    let sql = std::str::from_utf8(&stdin[0]).expect("UTF-8 SQL");
    assert!(sql.contains("ALTER ROLE stackctl_bill_db_role NOLOGIN;"));
    assert!(!sql.contains("DROP DATABASE"));
    assert!(!sql.contains("DROP ROLE"));
}

#[test]
fn orphaned_mysql_family_users_are_dropped_without_deleting_the_schema() {
    for (implementation, client, fingerprint_character) in
        [("mysql", "mysql", 'c'), ("mariadb", "mariadb", 'd')]
    {
        let fingerprint = format!("sha256:{}", fingerprint_character.to_string().repeat(64));
        let owned = owned_shared_container(&format!("{implementation}-container"), &fingerprint);
        let observed = ObservedContainer::new(owned.id().clone(), owned.metadata().labels());
        let resource = ResourceRecord::new(ResourceRecordOptions {
            resource_id: owned.id().as_str().to_owned(),
            installation_id: "install-1".to_owned(),
            kind: "shared_service".to_owned(),
            compatibility_fingerprint: fingerprint.clone(),
            project_id: None,
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            retention: ResourceRetention::Persistent,
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        });
        let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: "stackctl_bill_database".to_owned(),
            shared_resource_id: format!("{implementation}-data"),
            project_id: "bill".to_owned(),
            service_id: "database".to_owned(),
            kind: format!("{implementation}_database"),
            compatibility_fingerprint: fingerprint,
            desired_revision: "sha256:logical-v1".to_owned(),
            lifecycle: ResourceLifecycle::Orphaned,
            orphaned_at_unix_seconds: Some(12_345),
        });
        let credential = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!("bill/database/{implementation}"),
            project_id: Some("bill".to_owned()),
            service_id: "database".to_owned(),
            username: "st_bill_database".to_owned(),
            secret: "project-secret".to_owned(),
            lifecycle: CredentialLifecycle::Disabled,
        });
        let administrator = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!(
                "shared/{}/{}-bootstrap",
                fingerprint_character.to_string().repeat(64),
                implementation
            ),
            project_id: None,
            service_id: implementation.to_owned(),
            username: "root".to_owned(),
            secret: "administrator-secret".to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });
        let mut engine = RecordingOrphanAccessEngine {
            observed: vec![observed],
            state: crate::control_plane::engine::ContainerState::Running,
            started: Vec::new(),
            commands: RecordingOutputExecutor::new(vec![b"1\n".to_vec()]),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("test runtime");

        let revoked = runtime
            .block_on(revoke_orphaned_shared_access(
                &mut engine,
                OrphanedSharedAccessOptions {
                    resources: &[resource],
                    logical_resources: &[logical],
                    credentials: &[credential, administrator],
                    installation_id: "install-1",
                    schema_version: 8,
                    timeout: std::time::Duration::from_secs(15),
                },
            ))
            .expect("revoke orphaned MySQL-family access");
        runtime.block_on(tokio::task::yield_now());

        assert_eq!(revoked, 1);
        assert_eq!(
            engine.commands.requests.lock().expect("requests")[0][0],
            client
        );
        let stdin = engine.commands.stdin.lock().expect("recorded stdin");
        let sql = std::str::from_utf8(&stdin[0]).expect("UTF-8 SQL");
        assert!(sql.contains("DROP USER IF EXISTS 'st_bill_database'@'%';"));
        assert!(!sql.contains("DROP DATABASE"));
    }
}

#[test]
fn orphaned_mongodb_users_are_dropped_without_deleting_the_database() {
    let fingerprint = format!("sha256:{}", "e".repeat(64));
    let owned = owned_shared_container("mongodb-container", &fingerprint);
    let observed = ObservedContainer::new(owned.id().clone(), owned.metadata().labels());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: owned.id().as_str().to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "mongodb-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mongodb_database".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:logical-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
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
        credential_id: format!("shared/{}/mongodb-bootstrap", "e".repeat(64)),
        project_id: None,
        service_id: "mongodb".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let mut engine = RecordingOrphanAccessEngine {
        observed: vec![observed],
        state: crate::control_plane::engine::ContainerState::Running,
        started: Vec::new(),
        commands: RecordingOutputExecutor::new(vec![b"true\n".to_vec()]),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let revoked = runtime
        .block_on(revoke_orphaned_shared_access(
            &mut engine,
            OrphanedSharedAccessOptions {
                resources: &[resource],
                logical_resources: &[logical],
                credentials: &[credential, administrator],
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(15),
            },
        ))
        .expect("revoke orphaned MongoDB access");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(revoked, 1);
    assert_eq!(
        *engine.commands.requests.lock().expect("requests"),
        [vec![
            "mongosh".to_owned(),
            "--quiet".to_owned(),
            "--nodb".to_owned(),
            "--file".to_owned(),
            "/dev/stdin".to_owned(),
        ]]
    );
    let stdin = engine.commands.stdin.lock().expect("recorded stdin");
    let script = std::str::from_utf8(&stdin[0]).expect("UTF-8 script");
    assert!(script.contains("target.dropUser(\"st_bill_database\")"));
    assert!(!script.contains("dropDatabase"));
    assert!(!script.contains("administrator-secret"));
    assert!(!script.contains("project-secret"));
}

#[test]
fn orphaned_sql_server_logins_are_disabled_without_deleting_the_database() {
    let fingerprint = format!("sha256:{}", "f".repeat(64));
    let owned = owned_shared_container("sqlserver-container", &fingerprint);
    let observed = ObservedContainer::new(owned.id().clone(), owned.metadata().labels());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: owned.id().as_str().to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "sqlserver-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "sqlserver_database".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:logical-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/sqlserver".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "Project1!Secure".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/sqlserver-bootstrap", "f".repeat(64)),
        project_id: None,
        service_id: "sqlserver".to_owned(),
        username: "sa".to_owned(),
        secret: "Administrator1!Secure".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let mut engine = RecordingOrphanAccessEngine {
        observed: vec![observed],
        state: crate::control_plane::engine::ContainerState::Running,
        started: Vec::new(),
        commands: RecordingOutputExecutor::new(vec![b"true\n".to_vec()]),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let revoked = runtime
        .block_on(revoke_orphaned_shared_access(
            &mut engine,
            OrphanedSharedAccessOptions {
                resources: &[resource],
                logical_resources: &[logical],
                credentials: &[credential, administrator],
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(15),
            },
        ))
        .expect("revoke orphaned SQL Server access");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(revoked, 1);
    assert_eq!(
        engine.commands.requests.lock().expect("requests")[0][0],
        "/opt/mssql-tools18/bin/sqlcmd"
    );
    let stdin = engine.commands.stdin.lock().expect("recorded stdin");
    let sql = std::str::from_utf8(&stdin[0]).expect("UTF-8 SQL");
    assert!(sql.contains("ALTER LOGIN [st_bill_database] DISABLE;"));
    assert!(!sql.contains("DROP DATABASE"));
    assert!(!sql.contains("DROP LOGIN"));
    assert!(!sql.contains("Administrator1!Secure"));
    assert!(!sql.contains("Project1!Secure"));
}

#[test]
fn orphaned_minio_users_are_disabled_without_deleting_buckets_or_objects() {
    let fingerprint = format!("sha256:{}", "1".repeat(64));
    let owned = owned_shared_container("minio-container", &fingerprint);
    let observed = ObservedContainer::new(owned.id().clone(), owned.metadata().labels());
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: owned.id().as_str().to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        project_id: None,
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/s3/object-store".to_owned(),
        shared_resource_id: "minio-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "s3".to_owned(),
        kind: "minio_bucket_policy".to_owned(),
        compatibility_fingerprint: fingerprint,
        desired_revision: "sha256:logical-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/s3/object-store".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "s3".to_owned(),
        username: "st_bill_s3".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/minio-root", "1".repeat(64)),
        project_id: None,
        service_id: "minio".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let users = br#"{"status":"success","accessKey":"st_bill_s3","policyName":"stackctl-bill-s3","userStatus":"enabled"}
"#
    .to_vec();
    let mut engine = RecordingOrphanAccessEngine {
        observed: vec![observed],
        state: crate::control_plane::engine::ContainerState::Running,
        started: Vec::new(),
        commands: RecordingOutputExecutor::new(vec![users, Vec::new()]),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let revoked = runtime
        .block_on(revoke_orphaned_shared_access(
            &mut engine,
            OrphanedSharedAccessOptions {
                resources: &[resource],
                logical_resources: &[logical],
                credentials: &[credential, administrator],
                installation_id: "install-1",
                schema_version: 8,
                timeout: std::time::Duration::from_secs(15),
            },
        ))
        .expect("revoke orphaned MinIO access");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(revoked, 1);
    let requests = engine.commands.requests.lock().expect("requests");
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1],
        ["mc", "admin", "user", "disable", "stackctl", "st_bill_s3"]
    );
    assert!(requests.iter().flatten().all(|argument| {
        !matches!(
            argument.as_str(),
            "rm" | "remove" | "rb" | "DROP" | "administrator-secret"
        )
    }));
}

#[test]
fn mailpit_authentication_is_deterministic_attributed_and_secret_free() {
    let snapshot = MailpitAuthenticationSnapshot::new(vec![
        MailpitProjectDefinition::new(
            "shop",
            "mailpit",
            CredentialSecret::new("shop-secret".to_owned()),
        )
        .expect("shop Mailpit definition"),
        MailpitProjectDefinition::new(
            "bill",
            "mailpit",
            CredentialSecret::new("bill-secret".to_owned()),
        )
        .expect("bill Mailpit definition"),
    ])
    .expect("Mailpit authentication snapshot");
    let contents = std::str::from_utf8(snapshot.contents()).expect("UTF-8 password file");

    assert!(contents.starts_with("st_bill_mailpit:$2b$10$"));
    assert!(contents.contains("\nst_shop_mailpit:$2b$10$"));
    assert!(!contents.contains("secret"));
    assert_eq!(snapshot.project_count(), 2);
    assert!(snapshot.revision().starts_with("sha256:"));
    assert_eq!(
        format!("{snapshot:?}"),
        "MailpitAuthenticationSnapshot { project_count: 2 }"
    );
}

#[test]
fn mailpit_authentication_rejects_deterministic_identity_collisions() {
    let definitions = vec![
        MailpitProjectDefinition::new(
            "bill-api",
            "mailpit",
            CredentialSecret::new("one".to_owned()),
        )
        .expect("first Mailpit definition"),
        MailpitProjectDefinition::new(
            "bill",
            "api-mailpit",
            CredentialSecret::new("two".to_owned()),
        )
        .expect("second Mailpit definition"),
    ];

    let error = MailpitAuthenticationSnapshot::new(definitions).expect_err("identity collision");

    assert_eq!(
        error.to_string(),
        "Mailpit SMTP user 'st_bill_api_mailpit' is defined more than once"
    );
}

#[cfg(unix)]
#[test]
fn mailpit_authentication_store_is_private_hash_only_and_atomically_replaceable() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-mailpit-authentication-{}",
        std::process::id()
    ));
    drop(std::fs::remove_dir_all(&root));
    let initial = MailpitAuthenticationSnapshot::new(Vec::new()).expect("initial snapshot");
    let replacement = MailpitAuthenticationSnapshot::new(vec![
        MailpitProjectDefinition::new(
            "bill",
            "mailpit",
            CredentialSecret::new("project-secret".to_owned()),
        )
        .expect("Mailpit definition"),
    ])
    .expect("replacement snapshot");

    let stored = store_mailpit_authentication(&initial, &root).expect("initial store");
    let interrupted = root.join("mounted/.smtp-passwords.tmp");
    std::fs::write(&interrupted, "partial authentication")
        .expect("interrupted authentication write");
    store_mailpit_authentication(&replacement, &root).expect("replacement store");

    assert_eq!(stored.directory(), root);
    assert!(!interrupted.exists());
    assert_eq!(stored.mount_directory(), root.join("mounted"));
    assert_eq!(stored.password_file(), root.join("mounted/smtp-passwords"));
    let contents = std::fs::read_to_string(stored.password_file()).expect("password file");
    assert_eq!(contents.as_bytes(), replacement.contents());
    assert!(!contents.contains("project-secret"));
    assert_eq!(
        std::fs::metadata(stored.directory())
            .expect("private root")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(stored.password_file())
            .expect("password file")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );

    std::fs::remove_dir_all(&root).expect("remove Mailpit fixture");
}

#[test]
fn mailpit_materializes_one_persistent_attribution_enabled_instance() {
    let snapshot = MailpitAuthenticationSnapshot::new(vec![
        MailpitProjectDefinition::new(
            "bill",
            "mailpit",
            CredentialSecret::new("project-secret".to_owned()),
        )
        .expect("Mailpit definition"),
    ])
    .expect("Mailpit snapshot");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "mailpit",
        mailpit_profile("1"),
    )])
    .pop()
    .expect("shared Mailpit plan");
    let plan = MailpitSharedInstancePlan::new(
        &shared,
        MailpitSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mailpit-v1".to_owned(),
            authentication_directory: "/private/mailpit/mounted".into(),
            authentication_revision: snapshot.revision().to_owned(),
        },
    )
    .expect("Mailpit instance");
    let rotated_snapshot = MailpitAuthenticationSnapshot::new(vec![
        MailpitProjectDefinition::new(
            "bill",
            "mailpit",
            CredentialSecret::new("rotated-secret".to_owned()),
        )
        .expect("rotated Mailpit definition"),
    ])
    .expect("rotated Mailpit snapshot");
    let rotated = MailpitSharedInstancePlan::new(
        &shared,
        MailpitSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mailpit-v1".to_owned(),
            authentication_directory: "/private/mailpit/mounted".into(),
            authentication_revision: rotated_snapshot.revision().to_owned(),
        },
    )
    .expect("rotated Mailpit instance");

    assert!(plan.container().name().starts_with("stackctl-shared-"));
    assert_eq!(plan.smtp_port(), 1025);
    assert_eq!(plan.ui_port(), 8025);
    assert!(plan.volume().is_some());
    assert_eq!(plan.authentication_revision(), snapshot.revision());
    assert_eq!(plan.container().name(), rotated.container().name());
    assert_ne!(
        plan.container()
            .metadata()
            .labels()
            .get("dev.stackctl.desired"),
        rotated
            .container()
            .metadata()
            .labels()
            .get("dev.stackctl.desired")
    );
    assert_eq!(
        plan.container().environment().get("MP_TAGS_USERNAME"),
        Some(&"true".to_owned())
    );
    assert_eq!(
        plan.container().environment().get("MP_SMTP_AUTH_FILE"),
        Some(&"/etc/stackctl/mailpit/smtp-passwords".to_owned())
    );
    assert_eq!(
        plan.container()
            .health_check()
            .expect("Mailpit health check")
            .engine_test(),
        ["CMD", "/mailpit", "readyz"]
    );
    assert!(!format!("{:?}", plan.container()).contains("project-secret"));
}

#[test]
fn mailpit_project_resources_publish_attributed_smtp_and_deterministic_route() {
    let snapshot = MailpitAuthenticationSnapshot::new(vec![
        MailpitProjectDefinition::new(
            "bill",
            "mailpit",
            CredentialSecret::new("project-secret".to_owned()),
        )
        .expect("Mailpit definition"),
    ])
    .expect("Mailpit snapshot");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "mailpit",
        mailpit_profile("1"),
    )])
    .pop()
    .expect("shared Mailpit plan");
    let instance = MailpitSharedInstancePlan::new(
        &shared,
        MailpitSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mailpit-v1".to_owned(),
            authentication_directory: "/private/mailpit/mounted".into(),
            authentication_revision: snapshot.revision().to_owned(),
        },
    )
    .expect("Mailpit instance");

    let project = plan_mailpit_project_resources(
        "bill",
        "mailpit",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("Mailpit project resources");

    assert_eq!(project.definition().username(), "st_bill_mailpit");
    assert_eq!(project.credential().credential_id(), "bill/mailpit/mailpit");
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            (
                "MAIL_HOST".to_owned(),
                instance.container().name().to_owned()
            ),
            ("MAIL_MAILER".to_owned(), "smtp".to_owned()),
            ("MAIL_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("MAIL_PORT".to_owned(), "1025".to_owned()),
            ("MAIL_USERNAME".to_owned(), "st_bill_mailpit".to_owned()),
        ])
    );
    assert_eq!(project.route().domain(), "bill-mailpit.stackctl.localhost");
    assert_eq!(
        project.route().upstream(),
        format!("http://{}:8025", instance.container().name())
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[cfg(unix)]
#[test]
fn mailpit_reconciliation_publishes_authentication_before_start() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-mailpit-reconcile-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove stale Mailpit fixture");
    }
    let snapshot = MailpitAuthenticationSnapshot::new(vec![
        MailpitProjectDefinition::new(
            "bill",
            "mailpit",
            CredentialSecret::new("project-secret".to_owned()),
        )
        .expect("Mailpit definition"),
    ])
    .expect("Mailpit snapshot");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "mailpit",
        mailpit_profile("1"),
    )])
    .pop()
    .expect("shared Mailpit plan");
    let instance = MailpitSharedInstancePlan::new(
        &shared,
        MailpitSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mailpit-v1".to_owned(),
            authentication_directory: root.join("mounted"),
            authentication_revision: snapshot.revision().to_owned(),
        },
    )
    .expect("Mailpit instance");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_mailpit_authentication(
            &mut engine,
            &instance,
            &snapshot,
            &root,
            "install-1",
            8,
        ))
        .expect("reconcile Mailpit authentication");

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert!(root.join("mounted/smtp-passwords").is_file());
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );

    std::fs::remove_dir_all(&root).expect("remove Mailpit fixture");
}

#[cfg(unix)]
#[test]
fn mailpit_reconciliation_rejects_snapshot_or_mount_drift_before_writes() {
    let root =
        std::env::temp_dir().join(format!("stackctl-v8-mailpit-reject-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove stale Mailpit fixture");
    }
    let planned_snapshot =
        MailpitAuthenticationSnapshot::new(Vec::new()).expect("planned snapshot");
    let actual_snapshot = MailpitAuthenticationSnapshot::new(vec![
        MailpitProjectDefinition::new(
            "bill",
            "mailpit",
            CredentialSecret::new("project-secret".to_owned()),
        )
        .expect("Mailpit definition"),
    ])
    .expect("actual snapshot");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "mailpit",
        mailpit_profile("1"),
    )])
    .pop()
    .expect("shared Mailpit plan");
    let instance = MailpitSharedInstancePlan::new(
        &shared,
        MailpitSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mailpit-v1".to_owned(),
            authentication_directory: root.join("wrong"),
            authentication_revision: planned_snapshot.revision().to_owned(),
        },
    )
    .expect("Mailpit instance");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_mailpit_authentication(
            &mut engine,
            &instance,
            &actual_snapshot,
            &root,
            "install-1",
            8,
        ))
        .expect_err("snapshot drift");

    assert!(error.to_string().contains("authentication revision"));
    assert!(!root.exists());
    assert!(engine.operations.is_empty());
}

#[test]
fn redis_acl_snapshots_are_complete_deterministic_and_redacted() {
    let snapshot = RedisAclSnapshot::new(
        CredentialSecret::new("admin-secret".to_owned()),
        vec![
            RedisAclProject::new(
                "shop",
                "cache",
                CredentialSecret::new("shop-secret".to_owned()),
            )
            .expect("shop ACL"),
            RedisAclProject::new(
                "bill",
                "cache",
                CredentialSecret::new("bill-secret".to_owned()),
            )
            .expect("bill ACL"),
        ],
    )
    .expect("ACL snapshot");

    assert_eq!(
        snapshot.contents(),
        "user default off resetpass resetkeys resetchannels -@all\n\
         user stackctl_admin on resetpass #16175223c8ddce5ace0493c948569c211b03c4c6bb3d3e484434999448cffe01 resetkeys ~* resetchannels &* +@all\n\
         user st_bill_cache on resetpass #fdb34f0710b2f482f4eb9dded04a6777f64c7388e42b0553ad43b26e126b029c resetkeys ~stackctl:bill:cache:* resetchannels &stackctl:bill:cache:* -@all +@read +@write +@connection +@transaction +@pubsub +@scripting -@admin -@dangerous -scan -keys -randomkey\n\
         user st_shop_cache on resetpass #3c655a3878fd8e4145a5facca30188ce74792ddff5d57aaf2203bbe74a940cb5 resetkeys ~stackctl:shop:cache:* resetchannels &stackctl:shop:cache:* -@all +@read +@write +@connection +@transaction +@pubsub +@scripting -@admin -@dangerous -scan -keys -randomkey\n"
    );
    assert!(!snapshot.contents().contains("secret"));
    assert_eq!(
        format!("{snapshot:?}"),
        "RedisAclSnapshot { user_count: 3 }"
    );
    assert!(!format!("{snapshot:?}").contains("secret"));
}

#[test]
fn redis_tenant_acls_deny_cross_tenant_key_enumeration() {
    let project = RedisAclProject::new(
        "bill",
        "cache",
        CredentialSecret::new("bill-secret".to_owned()),
    )
    .expect("bill ACL");
    let snapshot = RedisAclSnapshot::new(
        CredentialSecret::new("admin-secret".to_owned()),
        vec![project],
    )
    .expect("ACL snapshot");

    for command in ["scan", "keys", "randomkey"] {
        assert!(
            snapshot.contents().contains(&format!("-{command}")),
            "tenant ACL must deny {command}"
        );
    }
}

#[test]
fn redis_acl_snapshots_reject_duplicate_project_users() {
    let projects = vec![
        RedisAclProject::new(
            "bill",
            "cache",
            CredentialSecret::new("first-secret".to_owned()),
        )
        .expect("first ACL"),
        RedisAclProject::new(
            "bill",
            "cache",
            CredentialSecret::new("second-secret".to_owned()),
        )
        .expect("second ACL"),
    ];

    let error = RedisAclSnapshot::new(CredentialSecret::new("admin".to_owned()), projects)
        .expect_err("duplicate ACL user");

    assert_eq!(
        error.to_string(),
        "Redis ACL user 'st_bill_cache' is defined more than once"
    );
}

#[cfg(unix)]
#[test]
fn redis_acl_store_atomically_replaces_a_private_directory_mounted_file() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-redis-acl-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    drop(std::fs::remove_dir_all(&root));
    let initial = RedisAclSnapshot::new(CredentialSecret::new("admin-one".to_owned()), Vec::new())
        .expect("initial ACL");
    let replacement =
        RedisAclSnapshot::new(CredentialSecret::new("admin-two".to_owned()), Vec::new())
            .expect("replacement ACL");

    let stored = store_redis_acl_snapshot(&initial, &root).expect("store initial ACL");
    let interrupted = root.join("mounted/.users.tmp");
    std::fs::write(&interrupted, "partial ACL").expect("interrupted ACL write");
    store_redis_acl_snapshot(&replacement, &root).expect("replace ACL");

    assert_eq!(stored.directory(), root);
    assert!(!interrupted.exists());
    assert_eq!(stored.mount_directory(), root.join("mounted"));
    assert_eq!(stored.acl_file(), root.join("mounted/users.acl"));
    assert_eq!(
        std::fs::read_to_string(stored.acl_file()).expect("stored ACL"),
        replacement.contents()
    );
    assert_eq!(
        std::fs::metadata(stored.directory())
            .expect("ACL directory")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(stored.mount_directory())
            .expect("ACL mount directory")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::metadata(stored.acl_file())
            .expect("ACL file")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    assert_eq!(
        std::fs::read_dir(stored.mount_directory())
            .expect("ACL mount directory entries")
            .count(),
        1
    );

    std::fs::remove_dir_all(&root).expect("remove ACL fixture");
}

#[test]
fn redis_and_valkey_materialize_as_separate_private_acl_backed_instances() {
    let redis_shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "cache",
        cache_profile("redis", "8", PersistenceMode::Persistent),
    )])
    .pop()
    .expect("shared Redis plan");
    let valkey_shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "shop",
        "cache",
        cache_profile("valkey", "9", PersistenceMode::Ephemeral),
    )])
    .pop()
    .expect("shared Valkey plan");
    let redis = RedisSharedInstancePlan::new(
        &redis_shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:redis-v1".to_owned(),
            acl_directory: "/private/redis-acl".into(),
            bootstrap_secret: CredentialSecret::new("redis-admin".to_owned()),
        },
    )
    .expect("Redis instance");
    let valkey = RedisSharedInstancePlan::new(
        &valkey_shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:valkey-v1".to_owned(),
            acl_directory: "/private/valkey-acl".into(),
            bootstrap_secret: CredentialSecret::new("valkey-admin".to_owned()),
        },
    )
    .expect("Valkey instance");

    assert_eq!(redis.flavor(), RedisFlavor::Redis);
    assert_eq!(valkey.flavor(), RedisFlavor::Valkey);
    assert_eq!(
        RedisFlavor::Redis.client_auth_environment_key(),
        "REDISCLI_AUTH"
    );
    assert_eq!(
        RedisFlavor::Valkey.client_auth_environment_key(),
        "REDISCLI_AUTH"
    );
    assert_ne!(redis.container().name(), valkey.container().name());
    assert_eq!(redis.acl_mount_target(), "/etc/stackctl/acl");
    assert_eq!(redis.acl_file(), "/etc/stackctl/acl/users.acl");
    assert_eq!(redis.data_mount_target(), "/data");
    assert!(redis.volume().is_some());
    assert!(valkey.volume().is_none());
    assert_eq!(
        redis.command_arguments(),
        [
            "redis-server",
            "--aclfile",
            "/etc/stackctl/acl/users.acl",
            "--appendonly",
            "yes",
        ]
    );
    assert_eq!(
        valkey.command_arguments(),
        [
            "valkey-server",
            "--aclfile",
            "/etc/stackctl/acl/users.acl",
            "--appendonly",
            "no",
        ]
    );
    assert!(format!("{:?}", redis.container()).contains("/private/redis-acl"));
    assert!(format!("{:?}", redis.container()).contains("read_only: true"));
    assert!(!format!("{:?}", redis.container()).contains("redis-admin"));
    assert_eq!(redis.bootstrap_credential().username(), "stackctl_admin");
    assert_eq!(redis.bootstrap_credential().project_id(), None);
}

#[test]
fn redis_project_resources_compose_acl_credential_and_managed_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "cache",
        cache_profile("redis", "8", PersistenceMode::Persistent),
    )])
    .pop()
    .expect("shared Redis plan");
    let instance = RedisSharedInstancePlan::new(
        &shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:redis-v1".to_owned(),
            acl_directory: "/private/redis-acl".into(),
            bootstrap_secret: CredentialSecret::new("redis-admin".to_owned()),
        },
    )
    .expect("Redis instance");

    let project = plan_redis_project_resources(
        "bill",
        "cache",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("Redis project resources");

    assert_eq!(project.acl().username(), "st_bill_cache");
    assert_eq!(project.acl().prefix(), "stackctl:bill:cache:");
    assert_eq!(project.credential().credential_id(), "bill/cache/redis");
    assert_eq!(project.credential().username(), "st_bill_cache");
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            (
                "HORIZON_PREFIX".to_owned(),
                "stackctl:bill:cache:horizon:".to_owned()
            ),
            (
                "REDIS_HOST".to_owned(),
                instance.container().name().to_owned()
            ),
            ("REDIS_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("REDIS_PORT".to_owned(), "6379".to_owned()),
            ("REDIS_PREFIX".to_owned(), "stackctl:bill:cache:".to_owned()),
            ("REDIS_USERNAME".to_owned(), "st_bill_cache".to_owned()),
        ])
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[cfg(unix)]
#[test]
fn redis_acl_reconciliation_publishes_snapshot_before_start_and_reload() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-redis-reconcile-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove stale Redis fixture");
    }
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "cache",
        cache_profile("redis", "8", PersistenceMode::Persistent),
    )])
    .pop()
    .expect("shared Redis plan");
    let instance = RedisSharedInstancePlan::new(
        &shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:redis-v1".to_owned(),
            acl_directory: root.join("mounted"),
            bootstrap_secret: CredentialSecret::new("redis-admin".to_owned()),
        },
    )
    .expect("Redis instance");
    let snapshot = RedisAclSnapshot::new(
        CredentialSecret::new("redis-admin".to_owned()),
        vec![
            RedisAclProject::new(
                "bill",
                "cache",
                CredentialSecret::new("project-secret".to_owned()),
            )
            .expect("Redis project ACL"),
        ],
    )
    .expect("Redis ACL snapshot");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_redis_acl_snapshot(
            &mut engine,
            &instance,
            &snapshot,
            &root,
            "install-1",
            8,
        ))
        .expect("reconcile Redis ACL snapshot");

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert!(root.join("mounted/users.acl").is_file());
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );
    assert!(
        engine
            .command_arguments
            .lock()
            .expect("Redis command arguments")
            .iter()
            .any(|arguments| arguments.ends_with(&["ACL".to_owned(), "LOAD".to_owned()]))
    );

    std::fs::remove_dir_all(&root).expect("remove Redis fixture");
}

#[cfg(unix)]
#[test]
fn redis_acl_reconciliation_rejects_unmanaged_mounts_before_writes() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-redis-mount-reject-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove stale Redis fixture");
    }
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "cache",
        cache_profile("redis", "8", PersistenceMode::Persistent),
    )])
    .pop()
    .expect("shared Redis plan");
    let instance = RedisSharedInstancePlan::new(
        &shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:redis-v1".to_owned(),
            acl_directory: root.join("wrong"),
            bootstrap_secret: CredentialSecret::new("redis-admin".to_owned()),
        },
    )
    .expect("Redis instance");
    let snapshot =
        RedisAclSnapshot::new(CredentialSecret::new("redis-admin".to_owned()), Vec::new())
            .expect("Redis ACL snapshot");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_redis_acl_snapshot(
            &mut engine,
            &instance,
            &snapshot,
            &root,
            "install-1",
            8,
        ))
        .expect_err("unmanaged ACL mount");

    assert!(
        error
            .to_string()
            .contains("ACL mount must use managed snapshot directory")
    );
    assert!(!root.exists());
    assert!(engine.operations.is_empty());
}

#[test]
fn redis_acl_reload_uses_environment_auth_and_checks_command_status() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "cache",
        cache_profile("redis", "8", PersistenceMode::Persistent),
    )])
    .pop()
    .expect("shared Redis plan");
    let instance = RedisSharedInstancePlan::new(
        &shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:redis-v1".to_owned(),
            acl_directory: "/private/redis-acl".into(),
            bootstrap_secret: CredentialSecret::new("redis-admin".to_owned()),
        },
    )
    .expect("Redis instance");
    let container = owned_shared_container("redis-container", "sha256:redis-8");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(reload_redis_acl(&executor, &container, &instance))
        .expect("reload Redis ACL");
    runtime.block_on(tokio::task::yield_now());

    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(request_debug.contains("argument_count: 6"));
    assert!(request_debug.contains("REDISCLI_AUTH"));
    assert!(!request_debug.contains("redis-admin"));
    assert!(executor.stdin.lock().expect("recorded stdin").is_empty());
}

#[test]
fn redis_acl_reload_waits_for_authenticated_readiness() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "cache",
        cache_profile("redis", "8", PersistenceMode::Persistent),
    )])
    .pop()
    .expect("shared Redis plan");
    let instance = RedisSharedInstancePlan::new(
        &shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:redis-v1".to_owned(),
            acl_directory: "/private/redis-acl".into(),
            bootstrap_secret: CredentialSecret::new("redis-admin".to_owned()),
        },
    )
    .expect("Redis instance");
    let container = owned_shared_container("redis-container", "sha256:redis-8");
    let executor = SequencedRedisExecutor::new([
        CommandStatus::Exited(1),
        CommandStatus::Exited(0),
        CommandStatus::Exited(0),
    ]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(reload_redis_acl(&executor, &container, &instance))
        .expect("reload Redis ACL after readiness");

    let requests = executor.requests.lock().expect("Redis requests");
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].last().map(String::as_str), Some("PING"));
    assert_eq!(requests[1].last().map(String::as_str), Some("PING"));
    assert_eq!(
        requests[2].iter().map(String::as_str).collect::<Vec<_>>(),
        ["redis-cli", "-e", "--user", "stackctl_admin", "ACL", "LOAD"]
    );
}

#[test]
fn postgres_logical_resources_use_deterministic_isolated_names_and_stdin() {
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL logical plan");

    assert_eq!(plan.database_name(), "stackctl_bill_database");
    assert_eq!(plan.role_name(), "stackctl_bill_database_role");
    assert_eq!(plan.credential_id(), "bill/database/postgresql");
    assert_eq!(
        plan.command_arguments(),
        [
            "psql",
            "--no-psqlrc",
            "--set=ON_ERROR_STOP=1",
            "--username=stackctl_admin",
            "--dbname=postgres",
        ]
    );
    assert!(plan.stdin_sql().contains("CREATE DATABASE"));
    assert!(plan.stdin_sql().contains("REVOKE ALL"));
    assert!(plan.stdin_sql().contains("project-secret"));
    assert!(!format!("{plan:?}").contains("project-secret"));
}

#[test]
fn postgres_logical_resources_fail_instead_of_shortening_identifiers() {
    let project_id = "a".repeat(50);

    let error = PostgresLogicalResourcePlan::new(
        &project_id,
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect_err("overlong PostgreSQL role");

    assert!(
        error
            .to_string()
            .contains("exceeds PostgreSQL's 63-byte limit")
    );
}

#[test]
fn postgres_shared_instances_materialize_one_private_persistent_container() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let plan = PostgresSharedInstancePlan::new(
        &shared,
        PostgresSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL instance plan");

    assert!(plan.container().name().starts_with("stackctl-shared-"));
    assert_eq!(plan.container().image(), shared.profile().image_digest());
    assert_eq!(
        plan.container()
            .metadata()
            .labels()
            .get("dev.stackctl.kind"),
        Some(&"shared_service".to_owned())
    );
    assert_eq!(plan.data_mount_target(), "/var/lib/postgresql/data");
    assert!(plan.volume().is_some());
    assert_eq!(plan.bootstrap_credential().project_id(), None);
    assert_eq!(plan.bootstrap_credential().secret(), "root-secret");
    assert!(!format!("{:?}", plan.bootstrap_credential()).contains("root-secret"));
    assert!(!format!("{:?}", plan.container()).contains("root-secret"));
}

#[test]
fn postgres_migration_target_is_separate_owned_and_retained() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let plan = PostgresSharedInstancePlan::new_migration_target(
        &shared,
        PostgresMigrationInstancePlanOptions {
            migration_id: "restore-bill-database-100".to_owned(),
            project_id: "bill".to_owned(),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:restore-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("restore-root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL migration target");

    assert_eq!(
        plan.container().name(),
        "stackctl-migration-restore-bill-database-100"
    );
    assert_eq!(plan.container().image(), shared.profile().image_digest());
    assert_eq!(plan.container().network(), Some("stackctl"));
    assert!(plan.container().port_bindings().is_empty());
    assert_eq!(
        plan.container().metadata().labels(),
        BTreeMap::from([
            (
                "dev.stackctl.compatibility.implementation".to_owned(),
                "postgresql".to_owned(),
            ),
            (
                "dev.stackctl.compatibility.major-version".to_owned(),
                "17".to_owned(),
            ),
            (
                "dev.stackctl.desired".to_owned(),
                "sha256:restore-v1".to_owned()
            ),
            (
                "dev.stackctl.fingerprint".to_owned(),
                shared.profile().fingerprint().as_str().to_owned(),
            ),
            (
                "dev.stackctl.installation".to_owned(),
                "install-1".to_owned()
            ),
            ("dev.stackctl.kind".to_owned(), "project_service".to_owned()),
            ("dev.stackctl.managed".to_owned(), "true".to_owned()),
            ("dev.stackctl.project".to_owned(), "bill".to_owned()),
            (
                "dev.stackctl.resource".to_owned(),
                "restore-bill-database-100".to_owned(),
            ),
            ("dev.stackctl.retention".to_owned(), "persistent".to_owned()),
            ("dev.stackctl.schema".to_owned(), "8".to_owned()),
        ])
    );
    assert_eq!(
        plan.volume().expect("retained target volume").name(),
        "stackctl-migration-restore-bill-database-100-data"
    );
    assert_eq!(plan.bootstrap_credential().project_id(), Some("bill"));
    assert_eq!(plan.bootstrap_credential().secret(), "restore-root-secret");
    assert!(!format!("{:?}", plan.container()).contains("restore-root-secret"));
}

#[test]
fn postgres_migration_target_reuses_its_durable_bootstrap_credential() {
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-postgres-migration-target-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let options = PostgresMigrationPreparationOptions {
        migration_id: "restore-bill-database-100",
        project_id: "bill",
        installation_id: "install-1",
        network_name: "stackctl",
        schema_version: 8,
        desired_revision: "sha256:restore-v1",
    };

    let first = prepare_postgres_migration_target(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        options,
    )
    .expect("first migration target preparation");
    let second = prepare_postgres_migration_target(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        options,
    )
    .expect("replayed migration target preparation");

    assert_eq!(first.container(), second.container());
    assert_eq!(first.volume(), second.volume());
    assert_eq!(first.bootstrap_credential(), second.bootstrap_credential());
    assert_eq!(store.credentials().expect("credentials").len(), 1);
    assert!(!format!("{:?}", first.container()).contains(first.bootstrap_credential().secret()));

    drop(store);
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove migration target state");
        }
    }
}

#[test]
fn postgres_migration_target_reconciliation_is_owned_ready_and_idempotent() {
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-postgres-migration-reconcile-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let mut engine = RecordingSharedVolumeEngine {
        health: crate::control_plane::engine::ContainerHealth::RunningUnverified,
        command_exits: Arc::new(Mutex::new(VecDeque::from([1, 0, 0]))),
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_postgres_migration_target(
            &mut store,
            &mut engine,
            &shared,
            &FixedCredentialEntropy(0x11),
            PostgresMigrationPreparationOptions {
                migration_id: "restore-bill-database-100",
                project_id: "bill",
                installation_id: "install-1",
                network_name: "stackctl",
                schema_version: 8,
                desired_revision: "sha256:restore-v1",
            },
        ))
        .expect("migration target reconciliation");

    assert_eq!(
        result.container().metadata().resource_id(),
        Some("restore-bill-database-100")
    );
    assert_eq!(
        result.volume().name(),
        "stackctl-migration-restore-bill-database-100-data"
    );
    assert_eq!(
        result.health(),
        crate::control_plane::engine::ContainerHealth::Healthy
    );
    assert_eq!(result.bootstrap_credential().project_id(), Some("bill"));
    assert_eq!(store.credentials().expect("credentials").len(), 1);
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );
    assert_eq!(
        engine
            .command_arguments
            .lock()
            .expect("PostgreSQL readiness commands")
            .len(),
        2,
        "migration target readiness must retry an authenticated protocol probe"
    );
    let first_secret = result.bootstrap_credential().secret().to_owned();
    let created_volume = engine.created[0].clone();
    let created_container = engine.created_containers[0].clone();
    engine.observed = vec![crate::control_plane::engine::ObservedVolume::new(
        created_volume.name(),
        created_volume.metadata().labels(),
    )];
    engine.observed_containers = vec![ObservedContainer::new(
        result.container().id().clone(),
        created_container.metadata().labels(),
    )];
    engine.state = crate::control_plane::engine::ContainerState::Running;
    engine.operations.clear();

    let replay = runtime
        .block_on(reconcile_postgres_migration_target(
            &mut store,
            &mut engine,
            &shared,
            &FixedCredentialEntropy(0x22),
            PostgresMigrationPreparationOptions {
                migration_id: "restore-bill-database-100",
                project_id: "bill",
                installation_id: "install-1",
                network_name: "stackctl",
                schema_version: 8,
                desired_revision: "sha256:restore-v1",
            },
        ))
        .expect("replayed migration target reconciliation");

    assert_eq!(replay.container(), result.container());
    assert_eq!(replay.volume(), result.volume());
    assert_eq!(replay.bootstrap_credential().secret(), first_secret);
    assert!(engine.operations.is_empty());
    assert_eq!(
        engine
            .command_arguments
            .lock()
            .expect("replayed PostgreSQL readiness commands")
            .len(),
        3,
        "replayed reconciliation must reverify service readiness"
    );
    assert_eq!(store.credentials().expect("replayed credentials").len(), 1);

    drop(store);
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove migration target state");
        }
    }
}

#[test]
fn postgres_migration_target_reconciliation_uses_protocol_readiness_over_engine_state() {
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-postgres-migration-unready-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_postgres_migration_target(
            &mut store,
            &mut engine,
            &shared,
            &FixedCredentialEntropy(0x11),
            PostgresMigrationPreparationOptions {
                migration_id: "restore-bill-database-100",
                project_id: "bill",
                installation_id: "install-1",
                network_name: "stackctl",
                schema_version: 8,
                desired_revision: "sha256:restore-v1",
            },
        ))
        .expect("authenticated PostgreSQL probe proves the starting target ready");

    assert_eq!(
        result.health(),
        crate::control_plane::engine::ContainerHealth::Healthy
    );
    assert_eq!(engine.created_containers.len(), 1);
    assert_eq!(
        engine
            .command_arguments
            .lock()
            .expect("PostgreSQL readiness command")
            .as_slice(),
        &[vec![
            "psql".to_owned(),
            "--no-psqlrc".to_owned(),
            "--set=ON_ERROR_STOP=1".to_owned(),
            "--username=stackctl_admin".to_owned(),
            "--dbname=postgres".to_owned(),
            "--command=SELECT 1".to_owned(),
        ]]
    );

    drop(store);
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove migration target state");
        }
    }
}

#[test]
fn postgres_eighteen_uses_the_new_parent_volume_mount() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "18"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let plan = PostgresSharedInstancePlan::new(
        &shared,
        PostgresSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL 18 instance plan");

    assert_eq!(plan.data_mount_target(), "/var/lib/postgresql");
}

#[test]
fn postgres_logical_provisioning_streams_secret_sql_and_checks_exit_status() {
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL logical plan");
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("owned metadata");
    let observed =
        ObservedContainer::new(ContainerId::new("postgres-container"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned container handle");
    let executor = RecordingPostgresExecutor::default();
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/postgres/bootstrap".to_owned(),
        project_id: None,
        service_id: "postgresql".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "root-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_postgres_logical_resource(
            &executor,
            &container,
            &plan,
            &administrator,
        ))
        .expect("provision PostgreSQL resource");
    runtime.block_on(tokio::task::yield_now());

    let stdin = executor.stdin.lock().expect("recorded stdin").clone();
    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(
        String::from_utf8(stdin)
            .expect("SQL UTF-8")
            .contains("project-secret")
    );
    assert!(!request_debug.contains("project-secret"));
    assert!(!request_debug.contains("root-secret"));
    assert!(request_debug.contains("PGPASSWORD"));
    assert!(request_debug.contains("argument_count: 5"));
}

#[test]
fn postgres_logical_provisioning_retries_the_initialization_server_transition() {
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL logical plan");
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("owned metadata");
    let observed =
        ObservedContainer::new(ContainerId::new("postgres-container"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned container handle");
    let executor = RecordingPostgresExecutor {
        statuses: Arc::new(Mutex::new(VecDeque::from([2, 0]))),
        ..RecordingPostgresExecutor::default()
    };
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/postgres/bootstrap".to_owned(),
        project_id: None,
        service_id: "postgresql".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "root-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_postgres_logical_resource(
            &executor,
            &container,
            &plan,
            &administrator,
        ))
        .expect("retry the transient PostgreSQL server transition");

    assert_eq!(executor.starts.load(Ordering::SeqCst), 2);
}

#[test]
fn postgres_project_resources_emit_stable_credentials_and_managed_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let instance = PostgresSharedInstancePlan::new(
        &shared,
        PostgresSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL instance plan");

    let project = plan_postgres_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL project resources");

    assert_eq!(
        project.credential().username(),
        "stackctl_bill_database_role"
    );
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            ("DB_CONNECTION".to_owned(), "pgsql".to_owned()),
            (
                "DB_DATABASE".to_owned(),
                "stackctl_bill_database".to_owned()
            ),
            ("DB_HOST".to_owned(), instance.container().name().to_owned()),
            ("DB_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("DB_PORT".to_owned(), "5432".to_owned()),
            (
                "DB_USERNAME".to_owned(),
                "stackctl_bill_database_role".to_owned()
            ),
        ])
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[test]
fn postgres_project_reconciliation_converges_instance_before_logical_resources() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let instance = PostgresSharedInstancePlan::new(
        &shared,
        PostgresSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL instance plan");
    let project = plan_postgres_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL project resources");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_postgres_project_resources(
            &mut engine,
            &instance,
            &project,
            "install-1",
            8,
        ))
        .expect("reconcile PostgreSQL project resources");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );
    assert!(
        String::from_utf8(
            engine
                .command_input
                .lock()
                .expect("PostgreSQL command input")
                .clone()
        )
        .expect("PostgreSQL SQL UTF-8")
        .contains("project-secret")
    );
    assert_eq!(project.environment().project_id(), "bill");
    let prepared = PreparedPostgresSharedInstance::new(instance, vec![project]);
    let logical = prepared.logical_record(&prepared.projects()[0], &result);
    assert_eq!(logical.logical_resource_id(), "stackctl_bill_database");
    assert_eq!(logical.kind(), "postgres_database_and_role");
}

#[test]
fn postgres_logical_failures_leave_shared_instance_and_volume_intact() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let instance = PostgresSharedInstancePlan::new(
        &shared,
        PostgresSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL instance plan");
    let project = plan_postgres_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL project resources");
    let mut engine = RecordingSharedVolumeEngine {
        command_exit: 1,
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_postgres_project_resources(
            &mut engine,
            &instance,
            &project,
            "install-1",
            8,
        ))
        .expect_err("logical provisioning failure");

    assert_eq!(
        error,
        SharedInfrastructureReconcileError::LogicalResourceDrift {
            resource_id: "stackctl_bill_database".to_owned(),
            detail: "PostgreSQL logical resource provisioning exited with status 1".to_owned(),
        }
    );
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );
    assert!(engine.removed.is_empty());
    assert!(engine.removed_containers.is_empty());
}

#[test]
fn mysql_and_mariadb_materialize_as_separate_private_instances() {
    let mysql_shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let maria_shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "shop",
        "database",
        sql_profile("mariadb", "11"),
    )])
    .pop()
    .expect("shared MariaDB plan");
    let mysql = MySqlSharedInstancePlan::new(
        &mysql_shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mysql-root".to_owned()),
        },
    )
    .expect("MySQL instance");
    let maria = MySqlSharedInstancePlan::new(
        &maria_shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mariadb-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("maria-root".to_owned()),
        },
    )
    .expect("MariaDB instance");

    assert_eq!(mysql.flavor(), MySqlFlavor::MySql);
    assert_eq!(maria.flavor(), MySqlFlavor::MariaDb);
    assert_ne!(mysql.container().name(), maria.container().name());
    assert_eq!(mysql.data_mount_target(), "/var/lib/mysql");
    assert!(mysql.volume().is_some());
    assert!(maria.volume().is_some());
    assert!(!format!("{:?}", mysql.bootstrap_credential()).contains("mysql-root"));
}

#[test]
fn mysql_migration_target_is_separate_owned_and_retained() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let target = MySqlSharedInstancePlan::new_migration_target(
        &shared,
        MySqlMigrationInstancePlanOptions {
            migration_id: "restore-42".to_owned(),
            project_id: "bill".to_owned(),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-target".to_owned(),
            bootstrap_secret: CredentialSecret::new("target-root".to_owned()),
        },
    )
    .expect("MySQL migration target");

    assert_eq!(target.container().name(), "stackctl-migration-restore-42");
    assert_eq!(target.container().metadata().project_id(), Some("bill"));
    assert_eq!(
        target.container().metadata().retention(),
        crate::control_plane::engine::RetentionClass::Persistent
    );
    assert_eq!(
        target.bootstrap_credential().credential_id(),
        "migration/restore-42/mysql-bootstrap"
    );
    assert!(target.volume().is_some());
    assert_ne!(
        target.container().name(),
        format!(
            "stackctl-shared-{}",
            shared
                .fingerprint()
                .as_str()
                .strip_prefix("sha256:")
                .expect("fingerprint")
        )
    );
}

#[test]
fn mysql_migration_target_reuses_its_durable_bootstrap_credential() {
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-mysql-migration-target-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let options = MySqlMigrationPreparationOptions {
        migration_id: "restore-42",
        project_id: "bill",
        installation_id: "install-1",
        network_name: "stackctl",
        schema_version: 8,
        desired_revision: "sha256:mysql-target",
    };

    let first =
        prepare_mysql_migration_target(&mut store, &shared, &FixedCredentialEntropy(0x11), options)
            .expect("first MySQL target preparation");
    let second =
        prepare_mysql_migration_target(&mut store, &shared, &FixedCredentialEntropy(0x22), options)
            .expect("replayed MySQL target preparation");

    assert_eq!(first.container(), second.container());
    assert_eq!(first.volume(), second.volume());
    assert_eq!(first.bootstrap_credential(), second.bootstrap_credential());
    assert_eq!(store.credentials().expect("credentials").len(), 1);

    drop(store);
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove MySQL target state");
        }
    }
}

#[test]
fn mysql_migration_target_uses_authenticated_protocol_readiness() {
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-mysql-migration-reconcile-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let mut engine = RecordingSharedVolumeEngine {
        health: crate::control_plane::engine::ContainerHealth::RunningUnverified,
        command_exits: Arc::new(Mutex::new(VecDeque::from([1, 0]))),
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_mysql_migration_target(
            &mut store,
            &mut engine,
            &shared,
            &FixedCredentialEntropy(0x11),
            MySqlMigrationPreparationOptions {
                migration_id: "restore-bill-database-100",
                project_id: "bill",
                installation_id: "install-1",
                network_name: "stackctl",
                schema_version: 8,
                desired_revision: "sha256:restore-v1",
            },
        ))
        .expect("authenticated MySQL probe proves the target ready");

    assert_eq!(
        result.health(),
        crate::control_plane::engine::ContainerHealth::Healthy
    );
    assert_eq!(
        engine
            .command_arguments
            .lock()
            .expect("MySQL readiness commands")
            .len(),
        2,
        "migration target readiness must retry an authenticated protocol probe"
    );

    drop(store);
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove MySQL target state");
        }
    }
}

#[test]
fn mysql_project_resources_isolate_schema_user_and_application_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let instance = MySqlSharedInstancePlan::new(
        &shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mysql-root".to_owned()),
        },
    )
    .expect("MySQL instance");
    let project = plan_mysql_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MySQL project resources");

    assert_eq!(project.logical().schema_name(), "stackctl_bill_database");
    assert_eq!(project.logical().username(), "st_bill_database");
    assert!(
        project
            .logical()
            .stdin_sql()
            .contains("GRANT ALL PRIVILEGES")
    );
    assert_eq!(
        project.environment().values().get("DB_CONNECTION"),
        Some(&"mysql".to_owned())
    );
    assert_eq!(
        project.environment().values().get("DB_HOST"),
        Some(&instance.container().name().to_owned())
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[test]
fn mysql_shared_reconciliation_records_the_physical_schema_identity() {
    let database_path = std::env::temp_dir().join(format!(
        "stackctl-mysql-logical-identity-{}-{}.sqlite3",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let mut store = SqliteStateStore::open(&database_path).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )]);
    let prepared = prepare_mysql_shared_instances(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        super::MySqlPreparationOptions {
            installation_id: "install-1",
            network_name: "stackctl",
            schema_version: 8,
        },
    )
    .expect("prepare MySQL instance");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_prepared_mysql_instance(
            &mut engine,
            &prepared[0],
            "install-1",
            8,
        ))
        .expect("reconcile prepared MySQL instance");

    assert_eq!(result.logical_resources().len(), 1);
    assert_eq!(
        result.logical_resources()[0].logical_resource_id(),
        "stackctl_bill_database",
        "backup and restore must address the physical MySQL schema"
    );

    drop(store);
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove MySQL logical identity state");
        }
    }
}

#[test]
fn mysql_project_reconciliation_converges_instance_and_schema_user() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let instance = MySqlSharedInstancePlan::new(
        &shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mysql-root".to_owned()),
        },
    )
    .expect("MySQL instance");
    let project = plan_mysql_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MySQL project resources");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_mysql_project_resources(
            &mut engine,
            &instance,
            &project,
            "install-1",
            8,
        ))
        .expect("reconcile MySQL project resources");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert!(
        String::from_utf8(
            engine
                .command_input
                .lock()
                .expect("MySQL command input")
                .clone()
        )
        .expect("MySQL SQL UTF-8")
        .contains("project-secret")
    );
}

#[test]
fn mysql_logical_provisioning_keeps_both_passwords_out_of_command_debug() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let instance = MySqlSharedInstancePlan::new(
        &shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mysql-root".to_owned()),
        },
    )
    .expect("MySQL instance");
    let project = plan_mysql_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MySQL project resources");
    let container = owned_shared_container("mysql-container", "sha256:mysql-8");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_mysql_logical_resource(
            &executor,
            &container,
            &instance,
            project.logical(),
        ))
        .expect("provision MySQL resource");
    runtime.block_on(tokio::task::yield_now());

    let stdin = executor.stdin.lock().expect("recorded stdin").clone();
    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(
        String::from_utf8(stdin)
            .expect("SQL UTF-8")
            .contains("project-secret")
    );
    assert!(request_debug.contains("MYSQL_PWD"));
    assert!(!request_debug.contains("mysql-root"));
    assert!(!request_debug.contains("project-secret"));
}

#[test]
fn mysql_logical_provisioning_retries_the_initialization_server_transition() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let instance = MySqlSharedInstancePlan::new(
        &shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mysql-root".to_owned()),
        },
    )
    .expect("MySQL instance");
    let project = plan_mysql_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MySQL project resources");
    let container = owned_shared_container("mysql-container", "sha256:mysql-8");
    let executor = RecordingPostgresExecutor {
        statuses: Arc::new(Mutex::new(VecDeque::from([1, 0]))),
        ..RecordingPostgresExecutor::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_mysql_logical_resource(
            &executor,
            &container,
            &instance,
            project.logical(),
        ))
        .expect("retry the transient MySQL server transition");

    assert_eq!(executor.starts.load(Ordering::SeqCst), 2);
}

#[test]
fn sql_server_instances_are_private_persistent_and_secret_safe() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("sqlserver", "2022"),
    )])
    .pop()
    .expect("shared SQL Server plan");
    let instance = SqlServerSharedInstancePlan::new(
        &shared,
        SqlServerSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:sqlserver-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("StrongRoot1".to_owned()),
            accept_eula: true,
            sqlcmd_path: "/opt/mssql-tools18/bin/sqlcmd".to_owned(),
        },
    )
    .expect("SQL Server instance");

    assert!(instance.container().port_bindings().is_empty());
    assert_eq!(instance.container().network(), Some("stackctl"));
    assert_eq!(instance.data_mount_target(), "/var/opt/mssql");
    assert_eq!(instance.container().volume_mounts().len(), 1);
    assert_eq!(
        instance
            .container()
            .health_check()
            .expect("SQL Server health")
            .engine_test(),
        vec![
            "CMD",
            "/opt/mssql-tools18/bin/sqlcmd",
            "-C",
            "-S",
            "127.0.0.1",
            "-U",
            "sa",
            "-Q",
            "SET NOCOUNT ON; SELECT 1",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    );
    assert_eq!(instance.bootstrap_credential().secret(), "StrongRoot1");
    assert!(!format!("{instance:?}").contains("StrongRoot1"));
}

#[test]
fn sql_server_migration_target_is_separate_owned_and_retained() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("sqlserver", "2022"),
    )])
    .pop()
    .expect("shared SQL Server plan");
    let target = SqlServerSharedInstancePlan::new_migration_target(
        &shared,
        SqlServerMigrationInstancePlanOptions {
            migration_id: "restore-42".to_owned(),
            project_id: "bill".to_owned(),
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:sqlserver-target".to_owned(),
            bootstrap_secret: CredentialSecret::new("StrongTarget1".to_owned()),
            accept_eula: true,
            sqlcmd_path: "/opt/mssql-tools18/bin/sqlcmd".to_owned(),
        },
    )
    .expect("SQL Server migration target");

    assert_eq!(target.container().name(), "stackctl-migration-restore-42");
    assert_eq!(target.container().metadata().project_id(), Some("bill"));
    assert_eq!(
        target.container().metadata().resource_id(),
        Some("restore-42")
    );
    assert_eq!(
        target.container().metadata().retention(),
        crate::control_plane::engine::RetentionClass::Persistent
    );
    assert_eq!(
        target.bootstrap_credential().credential_id(),
        "migration/restore-42/sqlserver-bootstrap"
    );
    assert_eq!(target.bootstrap_credential().project_id(), Some("bill"));
    assert!(target.volume().is_some());
    assert!(!format!("{target:?}").contains("StrongTarget1"));
}

#[test]
fn sql_server_migration_target_reuses_credential_and_converges_retained_service() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-sqlserver-migration-target-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create SQL Server target state directory");
    let mut store = SqliteStateStore::open(&root.join("state.sqlite3")).expect("state store");
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("sqlserver", "2022"),
    )])
    .pop()
    .expect("shared SQL Server plan");
    let options = SqlServerMigrationPreparationOptions {
        migration_id: "restore-42",
        project_id: "bill",
        installation_id: "install-1",
        network_name: "stackctl",
        schema_version: 8,
        desired_revision: "sha256:sqlserver-target",
    };
    let first = prepare_sql_server_migration_target(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x11),
        options,
    )
    .expect("first SQL Server target preparation");
    let second = prepare_sql_server_migration_target(
        &mut store,
        &shared,
        &FixedCredentialEntropy(0x22),
        options,
    )
    .expect("replayed SQL Server target preparation");
    assert_eq!(first.container(), second.container());
    assert_eq!(first.volume(), second.volume());
    assert_eq!(first.bootstrap_credential(), second.bootstrap_credential());
    assert_eq!(store.credentials().expect("credentials").len(), 1);

    let mut engine = RecordingSharedVolumeEngine {
        health: crate::control_plane::engine::ContainerHealth::Healthy,
        ..RecordingSharedVolumeEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");
    let result = runtime
        .block_on(reconcile_sql_server_migration_target(
            &mut store,
            &mut engine,
            &shared,
            &FixedCredentialEntropy(0x33),
            options,
        ))
        .expect("SQL Server target reconciliation");

    assert_eq!(
        result.container().metadata().resource_id(),
        Some("restore-42")
    );
    assert_eq!(result.volume().name(), "stackctl-migration-restore-42-data");
    assert_eq!(
        engine.operations,
        vec!["create-volume", "create-container", "start-container"]
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove SQL Server target state");
}

#[test]
fn sql_server_projects_get_database_login_and_managed_environment() {
    let instance = sql_server_instance();
    let project = plan_sql_server_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("StrongProject1".to_owned()),
    )
    .expect("SQL Server project resources");

    assert_eq!(project.logical().database_name(), "stackctl_bill_database");
    assert_eq!(project.logical().username(), "st_bill_database");
    assert!(project.logical().stdin_sql().contains("CREATE DATABASE"));
    assert!(project.logical().stdin_sql().contains("ALTER LOGIN"));
    assert!(
        project
            .logical()
            .stdin_sql()
            .contains("ALTER LOGIN [st_bill_database] ENABLE")
    );
    assert!(
        project
            .logical()
            .stdin_sql()
            .contains("\nGO\nUSE [stackctl_bill_database]")
    );
    assert!(project.logical().stdin_sql().contains("StrongProject1"));
    assert_eq!(
        project.environment().values().get("DB_CONNECTION"),
        Some(&"sqlsrv".to_owned())
    );
    assert_eq!(
        project.environment().values().get("DB_HOST"),
        Some(&instance.container().name().to_owned())
    );
    assert!(!format!("{project:?}").contains("StrongProject1"));
}

#[test]
fn sql_server_provisioning_keeps_passwords_out_of_command_debug() {
    let instance = sql_server_instance();
    let project = plan_sql_server_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("StrongProject1".to_owned()),
    )
    .expect("SQL Server project resources");
    let container = owned_shared_container("sqlserver-container", "sha256:sqlserver-2022");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_sql_server_logical_resource(
            &executor,
            &container,
            &instance,
            project.logical(),
        ))
        .expect("provision SQL Server resource");
    runtime.block_on(tokio::task::yield_now());

    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(request_debug.contains("SQLCMDPASSWORD"));
    assert!(!request_debug.contains("StrongRoot1"));
    assert!(!request_debug.contains("StrongProject1"));
}

#[test]
fn sql_server_reconciliation_converges_instance_database_and_login() {
    let instance = sql_server_instance();
    let project = plan_sql_server_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("StrongProject1".to_owned()),
    )
    .expect("SQL Server project resources");
    let mut engine = RecordingSharedVolumeEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_sql_server_project_resources(
            &mut engine,
            &instance,
            &project,
            "install-1",
            8,
        ))
        .expect("reconcile SQL Server project resources");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(result.action(), SharedServiceReconcileAction::Created);
    assert!(
        String::from_utf8(
            engine
                .command_input
                .lock()
                .expect("SQL Server command input")
                .clone()
        )
        .expect("SQL Server SQL UTF-8")
        .contains("StrongProject1")
    );
}

struct SequentialEntropy;

impl CredentialEntropy for SequentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index).expect("test entropy index");
        }

        Ok(())
    }
}

#[derive(Default)]
struct RecordingPostgresExecutor {
    stdin: Arc<Mutex<Vec<u8>>>,
    request_debug: Arc<Mutex<String>>,
    statuses: Arc<Mutex<VecDeque<i64>>>,
    starts: AtomicUsize,
}

struct SequencedRedisExecutor {
    requests: Arc<Mutex<Vec<Vec<String>>>>,
    statuses: Arc<Mutex<VecDeque<CommandStatus>>>,
    next_execution: AtomicUsize,
}

impl SequencedRedisExecutor {
    fn new(statuses: impl IntoIterator<Item = CommandStatus>) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            statuses: Arc::new(Mutex::new(statuses.into_iter().collect())),
            next_execution: AtomicUsize::new(1),
        }
    }
}

impl CommandExecutor for SequencedRedisExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.requests
            .lock()
            .expect("Redis request lock")
            .push(request.arguments().to_vec());
        let execution = self.next_execution.fetch_add(1, Ordering::SeqCst);
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = tokio::io::duplex(1024);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .await
                    .expect("read Redis readiness stdin");
            });
            let output: ContainerLogStream<'static> = Box::pin(stream::empty());

            Ok(CommandSession::new(
                CommandExecutionId::new(format!("redis-exec-{execution}")),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async move {
            self.statuses
                .lock()
                .expect("Redis status lock")
                .pop_front()
                .ok_or_else(|| crate::control_plane::engine::EngineError::Backend {
                    detail: "Redis test executor has no configured status".to_owned(),
                })
        })
    }
}

#[derive(Default)]
struct RecordingObjectStoreExecutor {
    inputs: Arc<Mutex<Vec<Vec<u8>>>>,
    requests: Arc<Mutex<Vec<Vec<String>>>>,
    request_debug: Arc<Mutex<Vec<String>>>,
}

impl CommandExecutor for RecordingObjectStoreExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        let inputs = Arc::clone(&self.inputs);
        self.requests
            .lock()
            .expect("object-store request lock")
            .push(request.arguments().to_vec());
        self.request_debug
            .lock()
            .expect("object-store debug lock")
            .push(format!("{request:?}"));
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = tokio::io::duplex(16 * 1024);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .await
                    .expect("read object-store stdin");
                inputs.lock().expect("object-store input lock").push(bytes);
            });
            let output: ContainerLogStream<'static> = Box::pin(stream::empty());

            Ok(CommandSession::new(
                CommandExecutionId::new("object-store-exec"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async { Ok(CommandStatus::Exited(0)) })
    }
}

struct RecordingOutputExecutor {
    outputs: Arc<Mutex<VecDeque<Vec<u8>>>>,
    requests: Arc<Mutex<Vec<Vec<String>>>>,
    stdin: Arc<Mutex<Vec<Vec<u8>>>>,
}

struct RecordingOrphanAccessEngine {
    observed: Vec<ObservedContainer>,
    state: crate::control_plane::engine::ContainerState,
    started: Vec<OwnedContainer>,
    commands: RecordingOutputExecutor,
}

impl crate::control_plane::engine::ContainerDiscovery for RecordingOrphanAccessEngine {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl crate::control_plane::engine::ContainerLifecycle for RecordingOrphanAccessEngine {
    fn create<'operation>(
        &'operation mut self,
        _options: &'operation crate::control_plane::engine::ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async {
            Err(crate::control_plane::engine::EngineError::Backend {
                detail: "unexpected container creation".to_owned(),
            })
        })
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.started.push(container.clone());
            Ok(())
        })
    }

    fn stop<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn remove<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, crate::control_plane::engine::ContainerState> {
        Box::pin(async { Ok(self.state) })
    }
}

impl CommandExecutor for RecordingOrphanAccessEngine {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.commands.start_command(container, request)
    }

    fn command_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.commands.command_status(execution_id, container_id)
    }
}

impl RecordingOutputExecutor {
    fn new(outputs: Vec<Vec<u8>>) -> Self {
        Self {
            outputs: Arc::new(Mutex::new(outputs.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
            stdin: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl CommandExecutor for RecordingOutputExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.requests
            .lock()
            .expect("request lock")
            .push(request.arguments().to_vec());
        let output = self
            .outputs
            .lock()
            .expect("output lock")
            .pop_front()
            .expect("configured command output");
        let container_id = container.id().clone();
        let stdin = Arc::clone(&self.stdin);

        Box::pin(async move {
            let (writer, mut reader) = tokio::io::duplex(1024);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .await
                    .expect("read command stdin");
                stdin.lock().expect("recorded stdin").push(bytes);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));

            Ok(CommandSession::new(
                CommandExecutionId::new("exec-output"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async { Ok(CommandStatus::Exited(0)) })
    }
}

impl CommandExecutor for RecordingPostgresExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        let stdin = Arc::clone(&self.stdin);
        let execution = self.starts.fetch_add(1, Ordering::SeqCst);
        *self.request_debug.lock().expect("request debug lock") = format!("{request:?}");
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = tokio::io::duplex(16 * 1024);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .await
                    .expect("read provisioning stdin");
                *stdin.lock().expect("stdin lock") = bytes;
            });
            let output: ContainerLogStream<'static> = Box::pin(stream::empty());

            Ok(CommandSession::new(
                CommandExecutionId::new(format!("postgres-exec-{execution}")),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async move {
            Ok(CommandStatus::Exited(
                self.statuses
                    .lock()
                    .expect("PostgreSQL status lock")
                    .pop_front()
                    .unwrap_or(0),
            ))
        })
    }
}

fn fingerprint(extensions: Vec<&str>, major_version: &str) -> CompatibilityFingerprint {
    CompatibilityFingerprint::from_options(postgres_options(extensions, major_version))
        .expect("valid compatibility fingerprint")
}

fn profile(extensions: Vec<&str>, major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(postgres_options(extensions, major_version))
        .expect("valid compatibility profile")
}

fn sql_profile(implementation: &str, major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: implementation.to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("{implementation}@sha256:{}", "b".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid SQL compatibility profile")
}

fn sql_server_instance() -> SqlServerSharedInstancePlan {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("sqlserver", "2022"),
    )])
    .pop()
    .expect("shared SQL Server plan");
    SqlServerSharedInstancePlan::new(
        &shared,
        SqlServerSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:sqlserver-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("StrongRoot1".to_owned()),
            accept_eula: true,
            sqlcmd_path: "/opt/mssql-tools18/bin/sqlcmd".to_owned(),
        },
    )
    .expect("SQL Server instance")
}

fn cache_profile(
    implementation: &str,
    major_version: &str,
    persistence: PersistenceMode,
) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: implementation.to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("{implementation}@sha256:{}", "c".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence,
        isolation: IsolationCapability::AclAndPrefix,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid cache compatibility profile")
}

fn rabbitmq_profile(major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "rabbitmq".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("rabbitmq@sha256:{}", "d".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::VirtualHostAndUser,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid RabbitMQ compatibility profile")
}

fn mailpit_profile(major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "mailpit".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("axllent/mailpit@sha256:{}", "e".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::from([
            ("smtp_authentication".to_owned(), "password_file".to_owned()),
            (
                "attribution".to_owned(),
                "authenticated_username".to_owned(),
            ),
        ]),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::None,
        platform_architecture: Some("linux/amd64".to_owned()),
    })
    .expect("Mailpit compatibility profile")
}

fn gotenberg_profile(major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "gotenberg".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("gotenberg/gotenberg@sha256:{}", "9".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Ephemeral,
        isolation: IsolationCapability::None,
        platform_architecture: Some("linux/amd64".to_owned()),
    })
    .expect("valid Gotenberg compatibility profile")
}

fn object_store_profile(implementation: &str, major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: implementation.to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("{implementation}@sha256:{}", "f".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::BucketAndPolicy,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid object-store compatibility profile")
}

fn object_store_project(
    implementation: &str,
) -> (ObjectStoreSharedInstancePlan, ObjectStoreProjectResources) {
    object_store_project_at(
        implementation,
        std::path::Path::new("/private/object-store/policies"),
    )
}

fn object_store_project_at(
    implementation: &str,
    policy_directory: &std::path::Path,
) -> (ObjectStoreSharedInstancePlan, ObjectStoreProjectResources) {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "s3",
        object_store_profile(implementation, "1"),
    )])
    .pop()
    .expect("shared object-store plan");
    let instance = ObjectStoreSharedInstancePlan::new(
        &shared,
        ObjectStoreSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:object-store-v1".to_owned(),
            policy_directory: policy_directory.to_path_buf(),
            root_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("object-store instance");
    let project = plan_object_store_project_resources(
        "bill",
        "s3",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("object-store project resources");

    (instance, project)
}

fn mongodb_profile(major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "mongodb".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("mongo@sha256:{}", "e".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid MongoDB compatibility profile")
}

fn mongodb_instance() -> (MongoDbSharedInstancePlan, OwnedContainer) {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        mongodb_profile("8"),
    )])
    .pop()
    .expect("shared MongoDB plan");
    let instance = MongoDbSharedInstancePlan::new(
        &shared,
        MongoDbSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mongodb-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mongo-root".to_owned()),
        },
    )
    .expect("MongoDB instance");
    let container = owned_shared_container("mongodb-container", "sha256:mongodb-8");

    (instance, container)
}

fn owned_shared_container(id: &str, fingerprint: &str) -> OwnedContainer {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: fingerprint.to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("owned metadata");
    let observed = ObservedContainer::new(ContainerId::new(id), metadata.labels());

    reconstruct_owned_container(&observed, "install-1", 8).expect("owned container handle")
}

fn owned_global_network() -> crate::control_plane::engine::ObservedNetwork {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
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
        crate::control_plane::engine::NetworkId::new("network-1"),
        metadata.labels(),
    )
}

#[derive(Clone, Default)]
struct RecordingBatchProvisioningEngine {
    active: Arc<AtomicUsize>,
    maximum_active: Arc<AtomicUsize>,
    delay: StdDuration,
}

impl crate::control_plane::engine::ImageResolver for RecordingBatchProvisioningEngine {
    fn ensure_image<'operation>(
        &'operation mut self,
        _reference: &'operation crate::control_plane::engine::ImmutableImageReference,
    ) -> EngineFuture<'operation, crate::control_plane::engine::ImageId> {
        Box::pin(async {
            crate::control_plane::engine::ImageId::new(format!("sha256:{}", "1".repeat(64)))
        })
    }
}

impl crate::control_plane::engine::ContainerLifecycle for RecordingBatchProvisioningEngine {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation crate::control_plane::engine::ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async move {
            reconstruct_owned_container(
                &ObservedContainer::new(
                    ContainerId::new(options.name()),
                    options.metadata().labels(),
                ),
                options.metadata().installation_id(),
                options.metadata().schema_version(),
            )
            .map_err(
                |ownership| crate::control_plane::engine::EngineError::Backend {
                    detail: format!("could not reconstruct provisioning job: {ownership:?}"),
                },
            )
        })
    }

    fn start<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn stop<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn remove<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, crate::control_plane::engine::ContainerState> {
        Box::pin(async { Ok(crate::control_plane::engine::ContainerState::Missing) })
    }
}

impl crate::control_plane::engine::ContainerCompletion for RecordingBatchProvisioningEngine {
    fn wait_for_success<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _timeout: StdDuration,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            let current = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum_active.fetch_max(current, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.active.fetch_sub(1, Ordering::SeqCst);

            Ok(())
        })
    }
}

struct RecordingSharedVolumeEngine {
    observed: Vec<crate::control_plane::engine::ObservedVolume>,
    created: Vec<crate::control_plane::engine::VolumeCreateOptions>,
    removed: Vec<crate::control_plane::engine::OwnedVolume>,
    observed_containers: Vec<ObservedContainer>,
    observed_networks: Vec<crate::control_plane::engine::ObservedNetwork>,
    created_containers: Vec<crate::control_plane::engine::ContainerCreateOptions>,
    started_containers: Vec<OwnedContainer>,
    stopped_containers: Vec<OwnedContainer>,
    removed_containers: Vec<OwnedContainer>,
    state: crate::control_plane::engine::ContainerState,
    health: crate::control_plane::engine::ContainerHealth,
    operations: Vec<&'static str>,
    reconnected_networks: Arc<Mutex<Vec<(String, String, String)>>>,
    command_input: Arc<Mutex<Vec<u8>>>,
    command_inputs: Arc<Mutex<Vec<Vec<u8>>>>,
    command_arguments: Arc<Mutex<Vec<Vec<String>>>>,
    command_exit: i64,
    command_exits: Arc<Mutex<VecDeque<i64>>>,
    completions: Arc<Mutex<usize>>,
    completion_fails: bool,
    completion_times_out: bool,
    ensured_images: Vec<String>,
}

impl Default for RecordingSharedVolumeEngine {
    fn default() -> Self {
        Self {
            observed: Vec::new(),
            created: Vec::new(),
            removed: Vec::new(),
            observed_containers: Vec::new(),
            observed_networks: vec![owned_global_network()],
            created_containers: Vec::new(),
            started_containers: Vec::new(),
            stopped_containers: Vec::new(),
            removed_containers: Vec::new(),
            state: crate::control_plane::engine::ContainerState::Missing,
            health: crate::control_plane::engine::ContainerHealth::Starting,
            operations: Vec::new(),
            reconnected_networks: Arc::new(Mutex::new(Vec::new())),
            command_input: Arc::new(Mutex::new(Vec::new())),
            command_inputs: Arc::new(Mutex::new(Vec::new())),
            command_arguments: Arc::new(Mutex::new(Vec::new())),
            command_exit: 0,
            command_exits: Arc::new(Mutex::new(VecDeque::new())),
            completions: Arc::new(Mutex::new(0)),
            completion_fails: false,
            completion_times_out: false,
            ensured_images: Vec::new(),
        }
    }
}

impl crate::control_plane::engine::ImageResolver for RecordingSharedVolumeEngine {
    fn ensure_image<'operation>(
        &'operation mut self,
        reference: &'operation crate::control_plane::engine::ImmutableImageReference,
    ) -> EngineFuture<'operation, crate::control_plane::engine::ImageId> {
        self.operations.push("ensure-image");
        self.ensured_images.push(reference.as_str().to_owned());
        Box::pin(async {
            crate::control_plane::engine::ImageId::new(format!("sha256:{}", "1".repeat(64)))
        })
    }
}

impl crate::control_plane::engine::VolumeDiscovery for RecordingSharedVolumeEngine {
    fn discover_managed_volumes(
        &self,
    ) -> EngineFuture<'_, Vec<crate::control_plane::engine::ObservedVolume>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl crate::control_plane::engine::VolumeManager for RecordingSharedVolumeEngine {
    fn create_volume<'operation>(
        &'operation mut self,
        options: &'operation crate::control_plane::engine::VolumeCreateOptions,
    ) -> EngineFuture<'operation, crate::control_plane::engine::OwnedVolume> {
        Box::pin(async move {
            self.operations.push("create-volume");
            self.created.push(options.clone());
            crate::control_plane::engine::reconstruct_owned_volume(
                &crate::control_plane::engine::ObservedVolume::new(
                    options.name(),
                    options.metadata().labels(),
                ),
                options.metadata().installation_id(),
                options.metadata().schema_version(),
            )
            .map_err(
                |ownership| crate::control_plane::engine::EngineError::Backend {
                    detail: format!("could not reconstruct created volume: {ownership:?}"),
                },
            )
        })
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        volume: &'operation crate::control_plane::engine::OwnedVolume,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(volume.clone());
            Ok(())
        })
    }
}

impl crate::control_plane::engine::ContainerDiscovery for RecordingSharedVolumeEngine {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        Box::pin(async { Ok(self.observed_containers.clone()) })
    }
}

impl crate::control_plane::engine::NetworkDiscovery for RecordingSharedVolumeEngine {
    fn discover_managed_networks(
        &self,
    ) -> EngineFuture<'_, Vec<crate::control_plane::engine::ObservedNetwork>> {
        Box::pin(async { Ok(self.observed_networks.clone()) })
    }
}

impl crate::control_plane::engine::ContainerNetworkIsolation for RecordingSharedVolumeEngine {
    fn disconnect_container_network<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _network: &'operation crate::control_plane::engine::OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn reconnect_container_network<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        network: &'operation crate::control_plane::engine::OwnedNetwork,
        alias: &'operation str,
    ) -> EngineFuture<'operation, ()> {
        let reconnected = Arc::clone(&self.reconnected_networks);
        Box::pin(async move {
            reconnected.lock().expect("reconnected networks").push((
                container.id().as_str().to_owned(),
                network.id().as_str().to_owned(),
                alias.to_owned(),
            ));
            Ok(())
        })
    }
}

impl crate::control_plane::engine::ContainerLifecycle for RecordingSharedVolumeEngine {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation crate::control_plane::engine::ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async move {
            self.operations.push("create-container");
            self.created_containers.push(options.clone());
            reconstruct_owned_container(
                &ObservedContainer::new(
                    ContainerId::new("created-shared-service"),
                    options.metadata().labels(),
                ),
                options.metadata().installation_id(),
                options.metadata().schema_version(),
            )
            .map_err(
                |ownership| crate::control_plane::engine::EngineError::Backend {
                    detail: format!("could not reconstruct shared service: {ownership:?}"),
                },
            )
        })
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.operations.push("start-container");
            self.started_containers.push(container.clone());
            Ok(())
        })
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.operations.push("stop-container");
            self.stopped_containers.push(container.clone());
            Ok(())
        })
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.operations.push("remove-container");
            self.removed_containers.push(container.clone());
            Ok(())
        })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, crate::control_plane::engine::ContainerState> {
        Box::pin(async { Ok(self.state) })
    }
}

impl crate::control_plane::engine::HealthObserver for RecordingSharedVolumeEngine {
    fn observe_health<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, crate::control_plane::engine::ContainerHealth> {
        Box::pin(async { Ok(self.health) })
    }
}

impl crate::control_plane::engine::ContainerCompletion for RecordingSharedVolumeEngine {
    fn wait_for_success<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _timeout: std::time::Duration,
    ) -> EngineFuture<'operation, ()> {
        let completions = Arc::clone(&self.completions);
        let completion_fails = self.completion_fails;
        let completion_times_out = self.completion_times_out;
        Box::pin(async move {
            *completions.lock().expect("completion count") += 1;
            if completion_times_out {
                Err(crate::control_plane::engine::EngineError::Timeout {
                    action: "wait for provisioning container".to_owned(),
                    timeout_milliseconds: 30_000,
                })
            } else if completion_fails {
                Err(crate::control_plane::engine::EngineError::ContainerExit {
                    container_id: "provisioning-job".to_owned(),
                    status_code: 1,
                })
            } else {
                Ok(())
            }
        })
    }
}

impl CommandExecutor for RecordingSharedVolumeEngine {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        let input = Arc::clone(&self.command_input);
        let inputs = Arc::clone(&self.command_inputs);
        let command_index = {
            let mut inputs = inputs.lock().expect("command inputs");
            let command_index = inputs.len();
            inputs.push(Vec::new());

            command_index
        };
        self.command_arguments
            .lock()
            .expect("command arguments")
            .push(request.arguments().to_vec());
        let container_id = container.id().clone();
        Box::pin(async move {
            let (writer, mut reader) = tokio::io::duplex(16 * 1024);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .await
                    .expect("read shared service command input");
                *input.lock().expect("command input") = bytes.clone();
                inputs.lock().expect("command inputs")[command_index] = bytes;
            });
            let output: ContainerLogStream<'static> = Box::pin(stream::empty());

            Ok(CommandSession::new(
                CommandExecutionId::new("shared-service-command"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        let status = self
            .command_exits
            .lock()
            .expect("command exit queue")
            .pop_front()
            .unwrap_or(self.command_exit);

        Box::pin(async move { Ok(CommandStatus::Exited(status)) })
    }
}

fn shared_volume_request() -> crate::control_plane::engine::VolumeCreateOptions {
    shared_volume_request_with_revision("sha256:desired-v1")
}

fn provisioning_job_request(
    installation_id: &str,
) -> crate::control_plane::engine::ContainerCreateOptions {
    provisioning_job_request_for_installation("object-store-bucket", installation_id)
}

fn provisioning_job_request_for(
    resource_id: &str,
) -> crate::control_plane::engine::ContainerCreateOptions {
    provisioning_job_request_for_installation(resource_id, "install-1")
}

fn provisioning_job_request_for_installation(
    resource_id: &str,
    installation_id: &str,
) -> crate::control_plane::engine::ContainerCreateOptions {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: installation_id.to_owned(),
            kind: crate::control_plane::engine::ResourceKind::ProvisioningJob,
            project_id: Some("bill".to_owned()),
            compatibility_fingerprint: "sha256:minio-client".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:bucket-v1".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Disposable,
        },
    )
    .and_then(|metadata| metadata.with_resource_id(resource_id))
    .expect("provisioning job metadata");
    crate::control_plane::engine::ContainerCreateOptions::new(
        format!("stackctl-job-bill-{resource_id}"),
        format!("minio/mc@sha256:{}", "f".repeat(64)),
        metadata,
    )
    .and_then(|request| request.with_network("stackctl"))
    .and_then(|request| request.with_platform("linux/amd64"))
    .and_then(|request| {
        request.with_command(vec![
            "mb".to_owned(),
            "--ignore-existing".to_owned(),
            "stackctl/stackctl-bill-object-store".to_owned(),
        ])
    })
    .and_then(|request| {
        request.with_environment(BTreeMap::from([(
            "MC_HOST_stackctl".to_owned(),
            "http://root:secret@stackctl-shared-minio:9000".to_owned(),
        )]))
    })
    .expect("provisioning job request")
}

fn shared_volume_request_with_revision(
    desired_revision: &str,
) -> crate::control_plane::engine::VolumeCreateOptions {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::Volume,
            project_id: None,
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            schema_version: 8,
            desired_revision: desired_revision.to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("shared volume metadata");

    crate::control_plane::engine::VolumeCreateOptions::new(
        "stackctl-shared-postgres-17-data",
        metadata,
    )
    .expect("shared volume request")
}

fn shared_container_request(
    desired_revision: &str,
) -> crate::control_plane::engine::ContainerCreateOptions {
    crate::control_plane::engine::ContainerCreateOptions::new(
        "stackctl-shared-postgres-17",
        format!("postgres@sha256:{}", "a".repeat(64)),
        shared_container_metadata("install-1", desired_revision),
    )
    .expect("shared container request")
}

fn shared_container_metadata(
    installation_id: &str,
    desired_revision: &str,
) -> crate::control_plane::engine::ManagedResourceMetadata {
    crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: installation_id.to_owned(),
            kind: crate::control_plane::engine::ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            schema_version: 8,
            desired_revision: desired_revision.to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("shared container metadata")
}

fn postgres_options(extensions: Vec<&str>, major_version: &str) -> CompatibilityFingerprintOptions {
    CompatibilityFingerprintOptions {
        implementation: "postgresql".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        extensions: extensions.into_iter().map(str::to_owned).collect(),
        immutable_settings: BTreeMap::from([(
            "authentication".to_owned(),
            "scram-sha-256".to_owned(),
        )]),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some("linux/arm64".to_owned()),
    }
}
