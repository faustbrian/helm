use super::{MigrationDecisionExecutionOptions, MigrationDecisionExecutionResult};
use crate::control_plane::daemon::ipc::IpcMigrationDecision;
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, ResourceKind,
    VolumeDiscovery, VolumeManager, reconstruct_owned_container,
};
use crate::control_plane::migration::{
    EnginePostgresSourceRetirement, MigrationCutoverPlan, MigrationRollbackPlan,
    MySqlMigrationOperations, MySqlMigrationOperationsOptions, PostgresMigrationOperations,
    PostgresMigrationOperationsOptions, PostgresSourceRetirementOptions, confirm_migration,
    rollback_migration,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, MySqlFlavor, MySqlMigrationPreparationOptions,
    MySqlSharedInstancePlan, MySqlSharedInstancePlanOptions, PostgresMigrationPreparationOptions,
    PostgresSharedInstancePlan, PostgresSharedInstancePlanOptions, plan_mysql_project_resources,
    plan_postgres_project_resources, reconcile_mysql_migration_target,
    reconcile_postgres_migration_target,
};
use crate::control_plane::state::{
    CredentialLifecycle, LogicalResourceRecord, LogicalResourceRecordOptions, MigrationPhase,
    MigrationRecord, MigrationRecordOptions, ResourceLifecycle, SqliteStateStore, StateStore,
};

/// Executes one exact operator decision from durable migration evidence.
pub(crate) async fn execute_queued_migration_decision<E, Entropy>(
    mut engine: E,
    entropy: Entropy,
    options: MigrationDecisionExecutionOptions,
) -> MigrationDecisionExecutionResult
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
    let outcome = execute(&mut engine, &entropy, &options)
        .await
        .map_err(|error| error.to_string());

    MigrationDecisionExecutionResult::new(options.operation, outcome)
}

async fn execute<E, Entropy>(
    engine: &mut E,
    entropy: &Entropy,
    options: &MigrationDecisionExecutionOptions,
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
    match decision_resource_kind(options)?.as_str() {
        "mysql_database" | "mariadb_database" => {
            execute_mysql_decision(engine, entropy, options).await
        }
        _ => execute_postgres_decision(engine, entropy, options).await,
    }
}

async fn execute_postgres_decision<E, Entropy>(
    engine: &mut E,
    entropy: &Entropy,
    options: &MigrationDecisionExecutionOptions,
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
        .map_err(|error| format!("could not open migration decision state: {error}"))?;
    let checkpoint = one(
        store
            .migrations()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|migration| {
                migration.migration_id() == options.operation.migration_id()
                    && migration.project_id() == options.operation.project_id()
            })
            .collect(),
        "durable migration checkpoint",
    )?;
    let project = one(
        store
            .projects()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|project| project.project_name() == options.operation.project_id())
            .collect(),
        "registered project",
    )?;
    checkpoint
        .target_resource_id()
        .ok_or_else(|| "migration decision checkpoint has no target identity".to_owned())?;
    let rollback_reference = checkpoint
        .rollback_reference()
        .ok_or_else(|| "migration decision checkpoint has no rollback identity".to_owned())?;
    let logical_resources = store
        .logical_resources()
        .map_err(|error| error.to_string())?;
    let source = one(
        logical_resources
            .iter()
            .filter(|logical| {
                logical.project_id() == options.operation.project_id()
                    && logical.shared_resource_id() == rollback_reference
                    && logical.compatibility_fingerprint()
                        == checkpoint.source_compatibility_fingerprint()
                    && logical.lifecycle() == ResourceLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source logical resource",
    )?;
    let target_logical = one(
        logical_resources
            .into_iter()
            .filter(|logical| {
                logical.project_id() == options.operation.project_id()
                    && logical.logical_resource_id()
                        == format!("{}/{}", source.project_id(), source.service_id())
                    && logical.shared_resource_id() != rollback_reference
                    && logical.compatibility_fingerprint()
                        == checkpoint.target_compatibility_fingerprint()
                    && logical.lifecycle() == ResourceLifecycle::Active
            })
            .collect(),
        "active target logical resource",
    )?;
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let source_credential = one(
        credentials
            .iter()
            .filter(|credential| {
                credential.project_id() == Some(source.project_id())
                    && credential.service_id() == source.service_id()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source credential",
    )?;
    let source_administrator_id = format!(
        "shared/{}/postgresql-bootstrap",
        checkpoint
            .source_compatibility_fingerprint()
            .strip_prefix("sha256:")
            .ok_or_else(|| "source compatibility fingerprint is malformed".to_owned())?
    );
    let source_administrator = one(
        credentials
            .into_iter()
            .filter(|credential| {
                credential.credential_id() == source_administrator_id
                    && credential.project_id().is_none()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .collect(),
        "active source administrator",
    )?;
    let source_container = owned_source_container(engine, options, &checkpoint).await?;
    let target = reconcile_postgres_migration_target(
        &mut store,
        engine,
        &options.shared,
        entropy,
        PostgresMigrationPreparationOptions {
            migration_id: checkpoint.migration_id(),
            project_id: checkpoint.project_id(),
            installation_id: &options.installation_id,
            network_name: &options.network_name,
            schema_version: options.schema_version,
            desired_revision: source.desired_revision(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let target_resources = plan_postgres_project_resources(
        source.project_id(),
        source.service_id(),
        target.plan(),
        CredentialSecret::new(source_credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    let source_plan = PostgresSharedInstancePlan::new(
        &options.shared,
        PostgresSharedInstancePlanOptions {
            installation_id: options.installation_id.clone(),
            network_name: options.network_name.clone(),
            schema_version: options.schema_version,
            desired_revision: source.desired_revision().to_owned(),
            bootstrap_secret: CredentialSecret::new(source_administrator.secret().to_owned()),
        },
    )
    .map_err(|error| error.to_string())?;
    let source_resources = plan_postgres_project_resources(
        source.project_id(),
        source.service_id(),
        &source_plan,
        CredentialSecret::new(source_credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    let target_environment = one(
        store
            .managed_environments()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|environment| environment.project_id() == checkpoint.project_id())
            .collect(),
        "active target environment",
    )?;
    let cutover = MigrationCutoverPlan::new(project.clone(), target_environment)
        .map_err(|error| error.to_string())?;
    let rollback = MigrationRollbackPlan::new(
        project,
        source_resources.environment().clone(),
        vec![logical_with_lifecycle(
            &target_logical,
            ResourceLifecycle::Retained,
        )],
    )
    .map_err(|error| error.to_string())?;
    let inventory = inventory_from_checkpoint(&checkpoint)?;
    let retirement = EnginePostgresSourceRetirement::new(
        engine,
        PostgresSourceRetirementOptions {
            source_container: &source_container,
            administrator: &source_administrator,
            source_credential: &source_credential,
            source_environment: source_resources.environment(),
            installation_id: &options.installation_id,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    let mut retirement = retirement;
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

    match options.operation.decision() {
        IpcMigrationDecision::Confirm => confirm_migration(
            &mut store,
            &inventory,
            &mut operations,
            options.updated_at_unix_seconds,
        )
        .await
        .map_err(|error| error.to_string()),
        IpcMigrationDecision::Rollback => rollback_migration(
            &mut store,
            &inventory,
            &mut operations,
            options.updated_at_unix_seconds,
        )
        .await
        .map_err(|error| error.to_string()),
    }
}

async fn execute_mysql_decision<E, Entropy>(
    engine: &mut E,
    entropy: &Entropy,
    options: &MigrationDecisionExecutionOptions,
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
        .map_err(|error| format!("could not open migration decision state: {error}"))?;
    let checkpoint = one(
        store
            .migrations()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|migration| {
                migration.migration_id() == options.operation.migration_id()
                    && migration.project_id() == options.operation.project_id()
            })
            .collect(),
        "durable migration checkpoint",
    )?;
    let project = one(
        store
            .projects()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|project| project.project_name() == options.operation.project_id())
            .collect(),
        "registered project",
    )?;
    let target_resource_id = checkpoint
        .target_resource_id()
        .ok_or_else(|| "migration decision checkpoint has no target identity".to_owned())?;
    let rollback_reference = checkpoint
        .rollback_reference()
        .ok_or_else(|| "migration decision checkpoint has no rollback identity".to_owned())?;
    let logical_resources = store
        .logical_resources()
        .map_err(|error| error.to_string())?;
    let source = one(
        logical_resources
            .iter()
            .filter(|logical| {
                logical.project_id() == options.operation.project_id()
                    && logical.shared_resource_id() == rollback_reference
                    && logical.compatibility_fingerprint()
                        == checkpoint.source_compatibility_fingerprint()
                    && logical.lifecycle() == ResourceLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source logical resource",
    )?;
    let target_logical = one(
        logical_resources
            .into_iter()
            .filter(|logical| {
                logical.project_id() == options.operation.project_id()
                    && logical.logical_resource_id()
                        == format!("{}/{}", source.project_id(), source.service_id())
                    && logical.shared_resource_id() != rollback_reference
                    && logical.compatibility_fingerprint()
                        == checkpoint.target_compatibility_fingerprint()
                    && logical.lifecycle() == ResourceLifecycle::Active
            })
            .collect(),
        "active target logical resource",
    )?;
    let flavor = mysql_flavor(source.kind())?;
    let implementation = mysql_implementation(flavor);
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let source_credential = one(
        credentials
            .iter()
            .filter(|credential| {
                credential.project_id() == Some(source.project_id())
                    && credential.service_id() == source.service_id()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .cloned()
            .collect(),
        "active source credential",
    )?;
    let source_administrator_id = format!(
        "shared/{}/{implementation}-bootstrap",
        checkpoint
            .source_compatibility_fingerprint()
            .strip_prefix("sha256:")
            .ok_or_else(|| "source compatibility fingerprint is malformed".to_owned())?
    );
    let source_administrator = one(
        credentials
            .into_iter()
            .filter(|credential| {
                credential.credential_id() == source_administrator_id
                    && credential.project_id().is_none()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .collect(),
        "active source administrator",
    )?;
    let source_container = owned_source_container(engine, options, &checkpoint).await?;
    let target = reconcile_mysql_migration_target(
        &mut store,
        engine,
        &options.shared,
        entropy,
        MySqlMigrationPreparationOptions {
            migration_id: checkpoint.migration_id(),
            project_id: checkpoint.project_id(),
            installation_id: &options.installation_id,
            network_name: &options.network_name,
            schema_version: options.schema_version,
            desired_revision: source.desired_revision(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let target_resources = plan_mysql_project_resources(
        source.project_id(),
        source.service_id(),
        target.plan(),
        CredentialSecret::new(source_credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    if target_resources.logical().schema_name() != target_resource_id {
        return Err("migration decision target schema differs from its checkpoint".to_owned());
    }
    let source_plan = MySqlSharedInstancePlan::new(
        &options.shared,
        MySqlSharedInstancePlanOptions {
            installation_id: options.installation_id.clone(),
            network_name: options.network_name.clone(),
            schema_version: options.schema_version,
            desired_revision: source.desired_revision().to_owned(),
            bootstrap_secret: CredentialSecret::new(source_administrator.secret().to_owned()),
        },
    )
    .map_err(|error| error.to_string())?;
    let source_resources = plan_mysql_project_resources(
        source.project_id(),
        source.service_id(),
        &source_plan,
        CredentialSecret::new(source_credential.secret().to_owned()),
    )
    .map_err(|error| error.to_string())?;
    let target_environment = one(
        store
            .managed_environments()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|environment| environment.project_id() == checkpoint.project_id())
            .collect(),
        "active target environment",
    )?;
    let cutover = MigrationCutoverPlan::new(project.clone(), target_environment)
        .map_err(|error| error.to_string())?;
    let rollback = MigrationRollbackPlan::new(
        project,
        source_resources.environment().clone(),
        vec![logical_with_lifecycle(
            &target_logical,
            ResourceLifecycle::Retained,
        )],
    )
    .map_err(|error| error.to_string())?;
    let inventory = inventory_from_checkpoint(&checkpoint)?;
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
            source_environment: source_resources.environment(),
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

    match options.operation.decision() {
        IpcMigrationDecision::Confirm => confirm_migration(
            &mut store,
            &inventory,
            &mut operations,
            options.updated_at_unix_seconds,
        )
        .await
        .map_err(|error| error.to_string()),
        IpcMigrationDecision::Rollback => rollback_migration(
            &mut store,
            &inventory,
            &mut operations,
            options.updated_at_unix_seconds,
        )
        .await
        .map_err(|error| error.to_string()),
    }
}

fn decision_resource_kind(options: &MigrationDecisionExecutionOptions) -> Result<String, String> {
    validate(options)?;
    let store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open migration decision state: {error}"))?;
    let checkpoint = one(
        store
            .migrations()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|migration| {
                migration.migration_id() == options.operation.migration_id()
                    && migration.project_id() == options.operation.project_id()
            })
            .collect(),
        "durable migration checkpoint",
    )?;
    let rollback_reference = checkpoint
        .rollback_reference()
        .ok_or_else(|| "migration decision checkpoint has no rollback identity".to_owned())?;
    let source = one(
        store
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|logical| {
                logical.project_id() == options.operation.project_id()
                    && logical.shared_resource_id() == rollback_reference
                    && logical.compatibility_fingerprint()
                        == checkpoint.source_compatibility_fingerprint()
                    && logical.lifecycle() == ResourceLifecycle::Active
            })
            .collect(),
        "active source logical resource",
    )?;

    Ok(source.kind().to_owned())
}

fn mysql_flavor(kind: &str) -> Result<MySqlFlavor, String> {
    match kind {
        "mysql_database" => Ok(MySqlFlavor::MySql),
        "mariadb_database" => Ok(MySqlFlavor::MariaDb),
        _ => Err(format!(
            "migration kind '{kind}' is not a MySQL-family resource"
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
    options: &MigrationDecisionExecutionOptions,
    checkpoint: &MigrationRecord,
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
                    == checkpoint.source_compatibility_fingerprint()
        })
        .collect::<Vec<_>>();

    one(matches, "owned source PostgreSQL container")
}

fn inventory_from_checkpoint(checkpoint: &MigrationRecord) -> Result<MigrationRecord, String> {
    MigrationRecord::new(MigrationRecordOptions {
        migration_id: checkpoint.migration_id().to_owned(),
        project_id: checkpoint.project_id().to_owned(),
        source_revision: checkpoint.source_revision().to_owned(),
        target_revision: checkpoint.target_revision().to_owned(),
        source_compatibility_fingerprint: checkpoint.source_compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: checkpoint.target_compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::Inventoried,
        backup_reference: None,
        backup_artifact_sha256: None,
        backup_artifact_size_bytes: None,
        target_resource_id: None,
        rollback_reference: checkpoint.rollback_reference().map(str::to_owned),
        updated_at_unix_seconds: checkpoint.updated_at_unix_seconds(),
    })
    .map_err(|error| error.to_string())
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

fn validate(options: &MigrationDecisionExecutionOptions) -> Result<(), String> {
    if options.installation_id.is_empty()
        || options.network_name.is_empty()
        || options.schema_version == 0
        || !options.state_database_path.is_absolute()
        || !options.backup_root.is_absolute()
        || options.updated_at_unix_seconds < 0
        || options.timeout.is_zero()
    {
        return Err("migration decision execution options are incomplete".to_owned());
    }

    Ok(())
}

fn one<T>(mut matches: Vec<T>, description: &str) -> Result<T, String> {
    match matches.len() {
        1 => Ok(matches.pop().expect("single match exists")),
        0 => Err(format!("migration decision found no exact {description}")),
        count => Err(format!(
            "migration decision found {count} matches for {description}; refusing to guess"
        )),
    }
}
