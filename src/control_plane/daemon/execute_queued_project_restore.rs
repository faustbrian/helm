use super::{
    PreparedDatabaseDump, PreparedDatabaseRollback, ProjectRestoreExecutionOptions,
    ProjectRestoreExecutionResult, execute_minio_project_restore, execute_project_volume_restore,
    execute_rabbitmq_project_restore, execute_redis_project_restore,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, ContainerNetworkIsolation,
    ContainerVolumeArchive, HealthObserver, NetworkDiscovery, ResourceKind, VolumeDiscovery,
    VolumeManager, reconstruct_owned_container,
};
use crate::control_plane::migration::{
    EnginePostgresSourceRetirement, MigrationCutoverPlan, MigrationRollbackPlan,
    MongoDbMigrationOperations, MongoDbMigrationOperationsOptions, MySqlBackupOptions,
    MySqlDumpRestoreOptions, MySqlMigrationOperations, MySqlMigrationOperationsOptions,
    PostgresMigrationOperations, PostgresMigrationOperationsOptions,
    PostgresSourceRetirementOptions, RecoveryPointRestoreOptions, SqlServerMigrationOperations,
    SqlServerMigrationOperationsOptions, backup_mysql_database, execute_recovery_point_restore,
    restore_mysql_dump,
};
use crate::control_plane::retention::open_stored_backup_artifact;
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, MongoDbMigrationPreparationOptions, MySqlFlavor,
    MySqlLogicalResourcePlan, MySqlMigrationPreparationOptions,
    PostgresMigrationPreparationOptions, SqlServerMigrationPreparationOptions,
    plan_mongodb_project_resources, plan_mysql_project_resources, plan_postgres_project_resources,
    plan_sql_server_project_resources, reconcile_mongodb_migration_target,
    reconcile_mysql_migration_target, reconcile_postgres_migration_target,
    reconcile_sql_server_migration_target,
};
use crate::control_plane::state::{
    CredentialLifecycle, LogicalResourceRecord, LogicalResourceRecordOptions, MigrationPhase,
    MigrationRecord, MigrationRecordOptions, ResourceLifecycle, SqliteStateStore, StateStore,
};

/// Resolves authoritative state and restores one exact cataloged recovery point.
pub(crate) async fn execute_queued_project_restore<E, Entropy>(
    mut engine: E,
    entropy: Entropy,
    options: ProjectRestoreExecutionOptions,
) -> ProjectRestoreExecutionResult
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + ContainerNetworkIsolation
        + ContainerVolumeArchive
        + HealthObserver
        + NetworkDiscovery
        + VolumeDiscovery
        + VolumeManager
        + Sync,
    Entropy: CredentialEntropy,
{
    let outcome = execute(&mut engine, &entropy, &options)
        .await
        .map_err(|error| error.to_string());

    ProjectRestoreExecutionResult::new(options.operation, outcome)
}

async fn execute<E, Entropy>(
    engine: &mut E,
    entropy: &Entropy,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationExecutionResult, String>
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + ContainerNetworkIsolation
        + ContainerVolumeArchive
        + HealthObserver
        + NetworkDiscovery
        + VolumeDiscovery
        + VolumeManager
        + Sync,
    Entropy: CredentialEntropy,
{
    if options.operation.dump_file().is_some() {
        return Box::pin(execute_mysql_dump(engine, options)).await;
    }
    match options.operation.kind() {
        "volume" => execute_project_volume_restore(engine, options).await,
        "minio_bucket_policy" => execute_minio_project_restore(engine, options).await,
        "rabbitmq_vhost_user" => execute_rabbitmq_project_restore(engine, options).await,
        "redis_acl_prefix" | "valkey_acl_prefix" => {
            execute_redis_project_restore(engine, options).await
        }
        "mongodb_database" => execute_mongodb(engine, entropy, options).await,
        "sqlserver_database" => execute_sql_server(engine, entropy, options).await,
        "mysql_database" | "mariadb_database" => execute_mysql(engine, entropy, options).await,
        _ => execute_postgres(engine, entropy, options).await,
    }
}

async fn execute_mysql_dump<E>(
    engine: &mut E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationExecutionResult, String>
where
    E: CommandExecutor + ContainerDiscovery,
{
    validate(options)?;
    let flavor = mysql_flavor(options.operation.kind())?;
    let implementation = mysql_implementation(flavor);
    let store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open restore state: {error}"))?;
    let source = one(
        store
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|logical| source_matches(logical, options))
            .collect(),
        "active source logical resource",
    )?;
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let credential = one(
        credentials
            .iter()
            .filter(|credential| {
                credential.project_id() == Some(options.operation.project_id())
                    && credential.service_id() == options.operation.service_id()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source credential",
    )?;
    let fingerprint_id = options
        .operation
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .ok_or_else(|| "restore compatibility fingerprint is malformed".to_owned())?;
    let administrator_id = format!("shared/{fingerprint_id}/{implementation}-bootstrap");
    let administrator = one(
        credentials
            .into_iter()
            .filter(|candidate| {
                candidate.credential_id() == administrator_id
                    && candidate.project_id().is_none()
                    && candidate.lifecycle() == CredentialLifecycle::Active
            })
            .collect(),
        "active source administrator",
    )?;
    let container = owned_source_container(engine, options).await?;
    let logical = MySqlLogicalResourcePlan::new(
        flavor,
        options.operation.project_id(),
        options.operation.service_id(),
        CredentialSecret::new(credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    if logical.schema_name() != source.logical_resource_id() {
        return Err("database dump target does not match the owned logical schema".to_owned());
    }
    let source_file = options
        .operation
        .dump_file()
        .ok_or_else(|| "database dump restore has no source file".to_owned())?;
    let dump = PreparedDatabaseDump::materialize(
        source_file,
        options.operation.dump_archive_entry(),
        &options.backup_root,
        options.operation.operation_id(),
    )?;
    if !options.operation.resets_database() {
        restore_mysql_dump(
            engine,
            &container,
            &MySqlDumpRestoreOptions {
                flavor,
                logical: &logical,
                credential: &credential,
                administrator: &administrator,
                file: dump.path(),
                reset: false,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string())?;

        return Ok(crate::control_plane::migration::MigrationExecutionResult::Confirmed);
    }

    let mut rollback =
        PreparedDatabaseRollback::prepare(&options.backup_root, options.operation.operation_id())?;
    let safety_backup = backup_mysql_database(
        engine,
        &container,
        &MySqlBackupOptions {
            flavor,
            logical_resource: &source,
            credential: &credential,
            database_name: logical.schema_name(),
            installation_id: &options.installation_id,
            created_at_unix_seconds: options.updated_at_unix_seconds,
            backup_root: rollback.root(),
            timeout: options.timeout,
        },
    )
    .await
    .map_err(|error| {
        format!("database reset safety backup failed before any destructive action: {error}")
    })?;
    let stored_safety_backup =
        open_stored_backup_artifact(safety_backup.reference()).map_err(|error| {
            format!("verified database reset safety backup is unavailable: {error}")
        })?;
    let restore = restore_mysql_dump(
        engine,
        &container,
        &MySqlDumpRestoreOptions {
            flavor,
            logical: &logical,
            credential: &credential,
            administrator: &administrator,
            file: dump.path(),
            reset: true,
            timeout: options.timeout,
        },
    )
    .await;
    let Err(restore_error) = restore else {
        return Ok(crate::control_plane::migration::MigrationExecutionResult::Confirmed);
    };
    let rollback_result = restore_mysql_dump(
        engine,
        &container,
        &MySqlDumpRestoreOptions {
            flavor,
            logical: &logical,
            credential: &credential,
            administrator: &administrator,
            file: stored_safety_backup.artifact_file(),
            reset: true,
            timeout: options.timeout,
        },
    )
    .await;
    match rollback_result {
        Ok(()) => Err(format!(
            "database dump restore failed; the previous database was restored successfully: {restore_error}"
        )),
        Err(rollback_error) => {
            rollback.preserve();

            Err(format!(
                "database dump restore failed and automatic rollback failed; verified safety backup retained at '{}': restore error: {restore_error}; rollback error: {rollback_error}",
                safety_backup.reference()
            ))
        }
    }
}

async fn execute_sql_server<E, Entropy>(
    engine: &mut E,
    entropy: &Entropy,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationExecutionResult, String>
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager
        + Sync,
    Entropy: CredentialEntropy,
{
    validate(options)?;
    let mut store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open restore state: {error}"))?;
    let project = one(
        store
            .projects()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|project| project.project_name() == options.operation.project_id())
            .collect(),
        "registered project",
    )?;
    let source = one(
        store
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|logical| source_matches(logical, options))
            .collect(),
        "active source logical resource",
    )?;
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let source_credential = one(
        credentials
            .iter()
            .filter(|credential| {
                credential.project_id() == Some(options.operation.project_id())
                    && credential.service_id() == options.operation.service_id()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source credential",
    )?;
    let fingerprint_id = options
        .operation
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .ok_or_else(|| "restore compatibility fingerprint is malformed".to_owned())?;
    let administrator_id = format!("shared/{fingerprint_id}/sqlserver-bootstrap");
    let source_administrator = one(
        credentials
            .into_iter()
            .filter(|credential| {
                credential.credential_id() == administrator_id
                    && credential.project_id().is_none()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .collect(),
        "active source administrator",
    )?;
    let source_environment = one(
        store
            .managed_environments()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|environment| environment.project_id() == options.operation.project_id())
            .collect(),
        "active source environment",
    )?;
    let source_container = owned_source_container(engine, options).await?;
    let target = reconcile_sql_server_migration_target(
        &mut store,
        engine,
        options.shared_target()?,
        entropy,
        SqlServerMigrationPreparationOptions {
            migration_id: options.operation.operation_id(),
            project_id: options.operation.project_id(),
            installation_id: &options.installation_id,
            network_name: &options.network_name,
            schema_version: options.schema_version,
            desired_revision: source.desired_revision(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let target_resources = plan_sql_server_project_resources(
        options.operation.project_id(),
        options.operation.service_id(),
        target.plan(),
        CredentialSecret::new(source_credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    let target_logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: format!(
            "{}/{}",
            options.operation.project_id(),
            options.operation.service_id()
        ),
        shared_resource_id: target.volume().name().to_owned(),
        project_id: options.operation.project_id().to_owned(),
        service_id: options.operation.service_id().to_owned(),
        kind: options.operation.kind().to_owned(),
        compatibility_fingerprint: options.operation.compatibility_fingerprint().to_owned(),
        desired_revision: target_resources.environment().revision().to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let cutover =
        MigrationCutoverPlan::new(project.clone(), target_resources.environment().clone())
            .map_err(|error| error.to_string())?;
    let rollback = MigrationRollbackPlan::new(
        project,
        source_environment.clone(),
        vec![logical_with_lifecycle(
            &target_logical,
            ResourceLifecycle::Retained,
        )],
    )
    .map_err(|error| error.to_string())?;
    let inventory = MigrationRecord::new(MigrationRecordOptions {
        migration_id: options.operation.operation_id().to_owned(),
        project_id: options.operation.project_id().to_owned(),
        source_revision: source.desired_revision().to_owned(),
        target_revision: target_resources.environment().revision().to_owned(),
        source_compatibility_fingerprint: source.compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: target_logical.compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::Inventoried,
        backup_reference: None,
        backup_artifact_sha256: None,
        backup_artifact_size_bytes: None,
        target_resource_id: None,
        rollback_reference: Some(source.shared_resource_id().to_owned()),
        updated_at_unix_seconds: options.updated_at_unix_seconds,
    })
    .map_err(|error| error.to_string())?;
    let mut operations = SqlServerMigrationOperations::new(
        engine,
        SqlServerMigrationOperationsOptions {
            source_container: &source_container,
            target_container: target.container(),
            source_logical_resource: &source,
            target_logical_resource: &target_logical,
            source_credential: &source_credential,
            target_credential: target_resources.credential(),
            source_administrator: &source_administrator,
            target_instance: target.plan(),
            source_environment: &source_environment,
            target_plan: target_resources.logical(),
            installation_id: &options.installation_id,
            backup_root: &options.backup_root,
            operation_unix_seconds: options.updated_at_unix_seconds,
            timeout: options.timeout,
            cutover,
            rollback,
        },
    )
    .map_err(|error| error.to_string())?;

    execute_recovery_point_restore(
        &mut store,
        &inventory,
        &mut operations,
        &RecoveryPointRestoreOptions {
            recovery_point_id: options.operation.recovery_point_id(),
            service_id: options.operation.service_id(),
            logical_resource_id: options.operation.logical_resource_id(),
            resource_kind: options.operation.kind(),
            updated_at_unix_seconds: options.updated_at_unix_seconds,
        },
    )
    .await
    .map_err(|error| error.to_string())
}

async fn execute_mongodb<E, Entropy>(
    engine: &mut E,
    entropy: &Entropy,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationExecutionResult, String>
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager
        + Sync,
    Entropy: CredentialEntropy,
{
    validate(options)?;
    let state_directory = options
        .state_database_path
        .parent()
        .ok_or_else(|| "restore state database has no parent directory".to_owned())?;
    let mut store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open restore state: {error}"))?;
    let project = one(
        store
            .projects()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|project| project.project_name() == options.operation.project_id())
            .collect(),
        "registered project",
    )?;
    let source = one(
        store
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|logical| source_matches(logical, options))
            .collect(),
        "active source logical resource",
    )?;
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let source_credential = one(
        credentials
            .iter()
            .filter(|credential| {
                credential.project_id() == Some(options.operation.project_id())
                    && credential.service_id() == options.operation.service_id()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source credential",
    )?;
    let fingerprint_id = options
        .operation
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .ok_or_else(|| "restore compatibility fingerprint is malformed".to_owned())?;
    let administrator_id = format!("shared/{fingerprint_id}/mongodb-bootstrap");
    let source_administrator = one(
        credentials
            .into_iter()
            .filter(|credential| {
                credential.credential_id() == administrator_id
                    && credential.project_id().is_none()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .collect(),
        "active source administrator",
    )?;
    let source_environment = one(
        store
            .managed_environments()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|environment| environment.project_id() == options.operation.project_id())
            .collect(),
        "active source environment",
    )?;
    let source_container = owned_source_container(engine, options).await?;
    let target = reconcile_mongodb_migration_target(
        &mut store,
        engine,
        options.shared_target()?,
        entropy,
        MongoDbMigrationPreparationOptions {
            migration_id: options.operation.operation_id(),
            project_id: options.operation.project_id(),
            installation_id: &options.installation_id,
            network_name: &options.network_name,
            schema_version: options.schema_version,
            desired_revision: source.desired_revision(),
            state_directory,
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let target_resources = plan_mongodb_project_resources(
        options.operation.project_id(),
        options.operation.service_id(),
        target.plan(),
        CredentialSecret::new(source_credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    let target_logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: format!(
            "{}/{}",
            options.operation.project_id(),
            options.operation.service_id()
        ),
        shared_resource_id: target.volume().name().to_owned(),
        project_id: options.operation.project_id().to_owned(),
        service_id: options.operation.service_id().to_owned(),
        kind: options.operation.kind().to_owned(),
        compatibility_fingerprint: options.operation.compatibility_fingerprint().to_owned(),
        desired_revision: target_resources.environment().revision().to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let retained_target = logical_with_lifecycle(&target_logical, ResourceLifecycle::Retained);
    let cutover =
        MigrationCutoverPlan::new(project.clone(), target_resources.environment().clone())
            .map_err(|error| error.to_string())?;
    let rollback =
        MigrationRollbackPlan::new(project, source_environment.clone(), vec![retained_target])
            .map_err(|error| error.to_string())?;
    let inventory = MigrationRecord::new(MigrationRecordOptions {
        migration_id: options.operation.operation_id().to_owned(),
        project_id: options.operation.project_id().to_owned(),
        source_revision: source.desired_revision().to_owned(),
        target_revision: target_resources.environment().revision().to_owned(),
        source_compatibility_fingerprint: source.compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: target_logical.compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::Inventoried,
        backup_reference: None,
        backup_artifact_sha256: None,
        backup_artifact_size_bytes: None,
        target_resource_id: None,
        rollback_reference: Some(source.shared_resource_id().to_owned()),
        updated_at_unix_seconds: options.updated_at_unix_seconds,
    })
    .map_err(|error| error.to_string())?;
    let mut operations = MongoDbMigrationOperations::new(
        engine,
        MongoDbMigrationOperationsOptions {
            source_container: &source_container,
            target_container: target.container(),
            source_logical_resource: &source,
            target_logical_resource: &target_logical,
            source_credential: &source_credential,
            target_credential: target_resources.credential(),
            source_administrator: &source_administrator,
            target_administrator: target.bootstrap_credential(),
            source_environment: &source_environment,
            target_plan: target_resources.logical(),
            installation_id: &options.installation_id,
            backup_root: &options.backup_root,
            operation_unix_seconds: options.updated_at_unix_seconds,
            timeout: options.timeout,
            cutover,
            rollback,
        },
    )
    .map_err(|error| error.to_string())?;

    execute_recovery_point_restore(
        &mut store,
        &inventory,
        &mut operations,
        &RecoveryPointRestoreOptions {
            recovery_point_id: options.operation.recovery_point_id(),
            service_id: options.operation.service_id(),
            logical_resource_id: options.operation.logical_resource_id(),
            resource_kind: options.operation.kind(),
            updated_at_unix_seconds: options.updated_at_unix_seconds,
        },
    )
    .await
    .map_err(|error| error.to_string())
}

async fn execute_postgres<E, Entropy>(
    engine: &mut E,
    entropy: &Entropy,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationExecutionResult, String>
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager
        + Sync,
    Entropy: CredentialEntropy,
{
    validate(options)?;
    let mut store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open restore state: {error}"))?;
    let project = one(
        store
            .projects()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|project| project.project_name() == options.operation.project_id())
            .collect(),
        "registered project",
    )?;
    let source = one(
        store
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|logical| source_matches(logical, options))
            .collect(),
        "active source logical resource",
    )?;
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let source_credential = one(
        credentials
            .iter()
            .filter(|credential| {
                credential.project_id() == Some(options.operation.project_id())
                    && credential.service_id() == options.operation.service_id()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source credential",
    )?;
    let fingerprint_id = options
        .operation
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .ok_or_else(|| "restore compatibility fingerprint is malformed".to_owned())?;
    let administrator_id = format!("shared/{fingerprint_id}/postgresql-bootstrap");
    let source_administrator = one(
        credentials
            .into_iter()
            .filter(|credential| {
                credential.credential_id() == administrator_id
                    && credential.project_id().is_none()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .collect(),
        "active source administrator",
    )?;
    let source_environment = one(
        store
            .managed_environments()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|environment| environment.project_id() == options.operation.project_id())
            .collect(),
        "active source environment",
    )?;
    let source_container = owned_source_container(engine, options).await?;
    let target = reconcile_postgres_migration_target(
        &mut store,
        engine,
        options.shared_target()?,
        entropy,
        PostgresMigrationPreparationOptions {
            migration_id: options.operation.operation_id(),
            project_id: options.operation.project_id(),
            installation_id: &options.installation_id,
            network_name: &options.network_name,
            schema_version: options.schema_version,
            desired_revision: source.desired_revision(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let target_resources = plan_postgres_project_resources(
        options.operation.project_id(),
        options.operation.service_id(),
        target.plan(),
        CredentialSecret::new(source_credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    let target_logical = logical_target(options, &target, &target_resources);
    let retained_target = logical_with_lifecycle(&target_logical, ResourceLifecycle::Retained);
    let cutover =
        MigrationCutoverPlan::new(project.clone(), target_resources.environment().clone())
            .map_err(|error| error.to_string())?;
    let rollback =
        MigrationRollbackPlan::new(project, source_environment.clone(), vec![retained_target])
            .map_err(|error| error.to_string())?;
    let inventory = MigrationRecord::new(MigrationRecordOptions {
        migration_id: options.operation.operation_id().to_owned(),
        project_id: options.operation.project_id().to_owned(),
        source_revision: source.desired_revision().to_owned(),
        target_revision: target_resources.environment().revision().to_owned(),
        source_compatibility_fingerprint: source.compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: target_logical.compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::Inventoried,
        backup_reference: None,
        backup_artifact_sha256: None,
        backup_artifact_size_bytes: None,
        target_resource_id: None,
        rollback_reference: Some(source.shared_resource_id().to_owned()),
        updated_at_unix_seconds: options.updated_at_unix_seconds,
    })
    .map_err(|error| error.to_string())?;
    let mut retirement = EnginePostgresSourceRetirement::new(
        engine,
        PostgresSourceRetirementOptions {
            source_container: &source_container,
            administrator: &source_administrator,
            source_credential: &source_credential,
            source_environment: &source_environment,
            installation_id: &options.installation_id,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    let mut operations = PostgresMigrationOperations::new(
        engine,
        &mut retirement,
        PostgresMigrationOperationsOptions {
            source_container: &source_container,
            target_container: target.container(),
            source_logical_resource: &source,
            target_logical_resource: &target_logical,
            source_credential: &source_credential,
            target_credential: target_resources.credential(),
            target_plan: target_resources.logical(),
            administrator: target.bootstrap_credential(),
            installation_id: &options.installation_id,
            source_database_name: source.logical_resource_id(),
            backup_root: &options.backup_root,
            operation_unix_seconds: options.updated_at_unix_seconds,
            timeout: options.timeout,
            cutover,
            rollback,
        },
    )
    .map_err(|error| error.to_string())?;

    execute_recovery_point_restore(
        &mut store,
        &inventory,
        &mut operations,
        &RecoveryPointRestoreOptions {
            recovery_point_id: options.operation.recovery_point_id(),
            service_id: options.operation.service_id(),
            logical_resource_id: options.operation.logical_resource_id(),
            resource_kind: options.operation.kind(),
            updated_at_unix_seconds: options.updated_at_unix_seconds,
        },
    )
    .await
    .map_err(|error| error.to_string())
}

async fn execute_mysql<E, Entropy>(
    engine: &mut E,
    entropy: &Entropy,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationExecutionResult, String>
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager
        + Sync,
    Entropy: CredentialEntropy,
{
    validate(options)?;
    let flavor = mysql_flavor(options.operation.kind())?;
    let implementation = mysql_implementation(flavor);
    let mut store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open restore state: {error}"))?;
    let project = one(
        store
            .projects()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|project| project.project_name() == options.operation.project_id())
            .collect(),
        "registered project",
    )?;
    let source = one(
        store
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|logical| source_matches(logical, options))
            .collect(),
        "active source logical resource",
    )?;
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let source_credential = one(
        credentials
            .iter()
            .filter(|credential| {
                credential.project_id() == Some(options.operation.project_id())
                    && credential.service_id() == options.operation.service_id()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source credential",
    )?;
    let fingerprint_id = options
        .operation
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .ok_or_else(|| "restore compatibility fingerprint is malformed".to_owned())?;
    let administrator_id = format!("shared/{fingerprint_id}/{implementation}-bootstrap");
    let source_administrator = one(
        credentials
            .into_iter()
            .filter(|credential| {
                credential.credential_id() == administrator_id
                    && credential.project_id().is_none()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .collect(),
        "active source administrator",
    )?;
    let source_environment = one(
        store
            .managed_environments()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|environment| environment.project_id() == options.operation.project_id())
            .collect(),
        "active source environment",
    )?;
    let source_container = owned_source_container(engine, options).await?;
    let target = reconcile_mysql_migration_target(
        &mut store,
        engine,
        options.shared_target()?,
        entropy,
        MySqlMigrationPreparationOptions {
            migration_id: options.operation.operation_id(),
            project_id: options.operation.project_id(),
            installation_id: &options.installation_id,
            network_name: &options.network_name,
            schema_version: options.schema_version,
            desired_revision: source.desired_revision(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let target_resources = plan_mysql_project_resources(
        options.operation.project_id(),
        options.operation.service_id(),
        target.plan(),
        CredentialSecret::new(source_credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    let target_logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: format!(
            "{}/{}",
            options.operation.project_id(),
            options.operation.service_id()
        ),
        shared_resource_id: target.volume().name().to_owned(),
        project_id: options.operation.project_id().to_owned(),
        service_id: options.operation.service_id().to_owned(),
        kind: options.operation.kind().to_owned(),
        compatibility_fingerprint: options.operation.compatibility_fingerprint().to_owned(),
        desired_revision: target_resources.environment().revision().to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let retained_target = logical_with_lifecycle(&target_logical, ResourceLifecycle::Retained);
    let cutover =
        MigrationCutoverPlan::new(project.clone(), target_resources.environment().clone())
            .map_err(|error| error.to_string())?;
    let rollback =
        MigrationRollbackPlan::new(project, source_environment.clone(), vec![retained_target])
            .map_err(|error| error.to_string())?;
    let inventory = MigrationRecord::new(MigrationRecordOptions {
        migration_id: options.operation.operation_id().to_owned(),
        project_id: options.operation.project_id().to_owned(),
        source_revision: source.desired_revision().to_owned(),
        target_revision: target_resources.environment().revision().to_owned(),
        source_compatibility_fingerprint: source.compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: target_logical.compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::Inventoried,
        backup_reference: None,
        backup_artifact_sha256: None,
        backup_artifact_size_bytes: None,
        target_resource_id: None,
        rollback_reference: Some(source.shared_resource_id().to_owned()),
        updated_at_unix_seconds: options.updated_at_unix_seconds,
    })
    .map_err(|error| error.to_string())?;
    let mut operations = MySqlMigrationOperations::new(
        engine,
        MySqlMigrationOperationsOptions {
            flavor,
            source_container: &source_container,
            target_container: target.container(),
            source_logical_resource: &source,
            target_logical_resource: &target_logical,
            source_credential: &source_credential,
            target_credential: target_resources.credential(),
            source_administrator: &source_administrator,
            source_environment: &source_environment,
            target_instance: target.plan(),
            target_plan: target_resources.logical(),
            installation_id: &options.installation_id,
            backup_root: &options.backup_root,
            operation_unix_seconds: options.updated_at_unix_seconds,
            timeout: options.timeout,
            cutover,
            rollback,
        },
    )
    .map_err(|error| error.to_string())?;

    execute_recovery_point_restore(
        &mut store,
        &inventory,
        &mut operations,
        &RecoveryPointRestoreOptions {
            recovery_point_id: options.operation.recovery_point_id(),
            service_id: options.operation.service_id(),
            logical_resource_id: options.operation.logical_resource_id(),
            resource_kind: options.operation.kind(),
            updated_at_unix_seconds: options.updated_at_unix_seconds,
        },
    )
    .await
    .map_err(|error| error.to_string())
}

fn mysql_flavor(kind: &str) -> Result<MySqlFlavor, String> {
    match kind {
        "mysql_database" => Ok(MySqlFlavor::MySql),
        "mariadb_database" => Ok(MySqlFlavor::MariaDb),
        _ => Err(format!(
            "restore kind '{kind}' is not a MySQL-family resource"
        )),
    }
}

const fn mysql_implementation(flavor: MySqlFlavor) -> &'static str {
    match flavor {
        MySqlFlavor::MySql => "mysql",
        MySqlFlavor::MariaDb => "mariadb",
    }
}

async fn owned_source_container<E>(
    engine: &E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::engine::OwnedContainer, String>
where
    E: ContainerDiscovery,
{
    let matches = engine
        .discover_managed()
        .await
        .map_err(|error| error.to_string())?
        .iter()
        .filter_map(|observed| {
            reconstruct_owned_container(observed, &options.installation_id, options.schema_version)
                .ok()
        })
        .filter(|container| {
            container.metadata().kind() == ResourceKind::SharedService
                && container.metadata().project_id().is_none()
                && container.metadata().compatibility_fingerprint()
                    == options.operation.compatibility_fingerprint()
        })
        .collect::<Vec<_>>();

    one(matches, "owned source PostgreSQL container")
}

fn logical_target(
    options: &ProjectRestoreExecutionOptions,
    target: &crate::control_plane::shared_infrastructure::PostgresMigrationTargetReconcileResult,
    resources: &crate::control_plane::shared_infrastructure::PostgresProjectResources,
) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: format!(
            "{}/{}",
            options.operation.project_id(),
            options.operation.service_id()
        ),
        shared_resource_id: target.volume().name().to_owned(),
        project_id: options.operation.project_id().to_owned(),
        service_id: options.operation.service_id().to_owned(),
        kind: options.operation.kind().to_owned(),
        compatibility_fingerprint: options.operation.compatibility_fingerprint().to_owned(),
        desired_revision: resources.environment().revision().to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn logical_with_lifecycle(
    logical: &LogicalResourceRecord,
    lifecycle: ResourceLifecycle,
) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical.logical_resource_id().to_owned(),
        shared_resource_id: logical.shared_resource_id().to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        desired_revision: logical.desired_revision().to_owned(),
        lifecycle,
        orphaned_at_unix_seconds: None,
    })
}

fn source_matches(
    logical: &LogicalResourceRecord,
    options: &ProjectRestoreExecutionOptions,
) -> bool {
    logical.logical_resource_id() == options.operation.logical_resource_id()
        && logical.project_id() == options.operation.project_id()
        && logical.service_id() == options.operation.service_id()
        && logical.kind() == options.operation.kind()
        && logical.compatibility_fingerprint() == options.operation.compatibility_fingerprint()
        && logical.lifecycle() == ResourceLifecycle::Active
}

fn validate(options: &ProjectRestoreExecutionOptions) -> Result<(), String> {
    if options.installation_id.is_empty()
        || options.network_name.is_empty()
        || options.schema_version == 0
        || !options.state_database_path.is_absolute()
        || !options.backup_root.is_absolute()
        || options.updated_at_unix_seconds < 0
        || options.timeout.is_zero()
        || options.shared_target()?.fingerprint().as_str()
            != options.operation.compatibility_fingerprint()
    {
        return Err("project restore execution options are incomplete or incompatible".to_owned());
    }

    Ok(())
}

fn one<T>(mut matches: Vec<T>, description: &str) -> Result<T, String> {
    match matches.len() {
        1 => matches
            .pop()
            .ok_or_else(|| format!("project restore lost its exact {description}")),
        0 => Err(format!("project restore found no exact {description}")),
        count => Err(format!(
            "project restore found {count} matches for {description}; refusing to guess"
        )),
    }
}
