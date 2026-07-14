use super::{RegisterAcceptedV7LogicalDataAdaptersOptions, V7LogicalDataCredential};
use crate::control_plane::engine::{
    CommandExecutor, ContainerCreateOptions, OwnedContainer, V7ContainerCommandExecutor,
    V7ContainerRetirement,
};
use crate::control_plane::migration::{
    EngineV7MinioSourceRetirement, EngineV7MongoDbSourceRetirement, EngineV7MySqlSourceRetirement,
    EngineV7PostgresSourceRetirement, EngineV7RabbitMqSourceRetirement,
    EngineV7RedisSourceRetirement, EngineV7SqlServerSourceRetirement,
    V7LogicalDataMigrationAdapterOptions, V7LogicalDataMigrationSource, V7MigrationAdapterRegistry,
    V7MinioMigrationProvider, V7MinioMigrationProviderOptions, V7MongoDbMigrationProvider,
    V7MongoDbMigrationProviderOptions, V7MySqlMigrationProvider, V7MySqlMigrationProviderOptions,
    V7PostgresMigrationProvider, V7PostgresMigrationProviderOptions, V7RabbitMqMigrationProvider,
    V7RabbitMqMigrationProviderOptions, V7RedisMigrationProvider, V7RedisMigrationProviderOptions,
    V7SqlServerMigrationProvider, V7SqlServerMigrationProviderOptions,
    register_v7_logical_data_migration_adapter,
};
use crate::control_plane::shared_infrastructure::{ObjectStoreFlavor, PreparedSharedInstance};
use crate::control_plane::state::{
    LogicalResourceRecord, ResourceLifecycle, V7MigrationExecutionRecord,
};

/// Binds every accepted logical source to one exact reconciled v8 target.
pub(crate) fn register_accepted_v7_logical_data_adapters<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    options: RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<usize, String>
where
    E: CommandExecutor + V7ContainerCommandExecutor + V7ContainerRetirement + Send + Sync,
{
    validate_options(execution, &options)?;
    let mut registered = 0;
    for input in options.inputs {
        let source = input.source();
        let did_register = match source.driver() {
            "postgres" => {
                register_postgres(registry, execution, source, input.credential(), &options)
            }
            "mysql" => register_mysql(registry, execution, source, input.credential(), &options),
            "mongodb" => {
                register_mongodb(registry, execution, source, input.credential(), &options)
            }
            "sqlserver" => {
                register_sql_server(registry, execution, source, input.credential(), &options)
            }
            "redis" | "valkey" => {
                register_redis(registry, execution, source, input.credential(), &options)
            }
            "minio" => register_minio(registry, execution, source, input.credential(), &options),
            "rabbitmq" => {
                register_rabbitmq(registry, execution, source, input.credential(), &options)
            }
            driver => {
                return Err(format!(
                    "accepted v7 logical driver '{driver}' is unsupported"
                ));
            }
        }?;
        if !did_register {
            return Err(format!(
                "accepted v7 logical source '{}:{}' has no execution checkpoint",
                source.project_id(),
                source.service_id()
            ));
        }
        registered += 1;
    }

    Ok(registered)
}

fn validate_options<E>(
    execution: &V7MigrationExecutionRecord,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'_, E>,
) -> Result<(), String> {
    if execution.project_id() != options.accepted.project_id()
        || execution.canonical_project_path() != options.accepted.canonical_project_path()
        || execution.evidence_revision() != options.accepted.evidence_revision()
    {
        return Err("accepted v7 logical composition identity is inconsistent".to_owned());
    }
    if options.installation_id.is_empty()
        || !options.backup_root.is_absolute()
        || options.created_at_unix_seconds < 0
        || options.verified_at_unix_seconds < options.created_at_unix_seconds
        || options.timeout.is_zero()
    {
        return Err("accepted v7 logical composition options are invalid".to_owned());
    }

    Ok(())
}

fn register_postgres<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    source: &'operation V7LogicalDataMigrationSource,
    credential: &'operation V7LogicalDataCredential,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<bool, String>
where
    E: CommandExecutor + V7ContainerCommandExecutor + V7ContainerRetirement + Send + Sync,
{
    let V7LogicalDataCredential::Postgres(source_credential) = credential else {
        return Err(credential_mismatch(source, credential));
    };
    let (prepared, project) = one_target(
        options.prepared.iter().flat_map(|prepared| match prepared {
            PreparedSharedInstance::Postgres(prepared) => prepared
                .projects()
                .iter()
                .filter(|project| {
                    matches_source(
                        source,
                        project.logical().project_id(),
                        project.logical().service_id(),
                    )
                })
                .map(|project| (prepared, project))
                .collect(),
            _ => Vec::new(),
        }),
        source,
    )?;
    let target_container = target_container(prepared.instance().container(), options, source)?;
    let target = target_logical_resource(source, prepared.instance().container(), options)?;
    let provider = V7PostgresMigrationProvider::new(
        options.engine,
        EngineV7PostgresSourceRetirement::new(options.engine),
        V7PostgresMigrationProviderOptions {
            accepted: options.accepted,
            source,
            source_credential,
            target_container,
            target_logical_resource: target,
            target_credential: project.credential(),
            target_plan: project.logical(),
            administrator: prepared.instance().bootstrap_credential(),
            installation_id: options.installation_id,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    register_provider(registry, execution, source, options, provider)
}

fn register_mysql<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    source: &'operation V7LogicalDataMigrationSource,
    credential: &'operation V7LogicalDataCredential,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<bool, String>
where
    E: CommandExecutor + V7ContainerCommandExecutor + V7ContainerRetirement + Send + Sync,
{
    let V7LogicalDataCredential::MySql(source_credential) = credential else {
        return Err(credential_mismatch(source, credential));
    };
    let (prepared, project) = one_target(
        options.prepared.iter().flat_map(|prepared| match prepared {
            PreparedSharedInstance::MySql(prepared) => prepared
                .projects()
                .iter()
                .filter(|project| {
                    matches_source(
                        source,
                        project.credential().project_id().unwrap_or_default(),
                        project.credential().service_id(),
                    )
                })
                .map(|project| (prepared, project))
                .collect(),
            _ => Vec::new(),
        }),
        source,
    )?;
    let target_container = target_container(prepared.instance().container(), options, source)?;
    let target = target_logical_resource(source, prepared.instance().container(), options)?;
    let provider = V7MySqlMigrationProvider::new(
        options.engine,
        EngineV7MySqlSourceRetirement::new(options.engine),
        V7MySqlMigrationProviderOptions {
            accepted: options.accepted,
            source,
            flavor: prepared.instance().flavor(),
            source_credential,
            target_container,
            target_logical_resource: target,
            target_credential: project.credential(),
            target_plan: project.logical(),
            administrator: prepared.instance().bootstrap_credential(),
            installation_id: options.installation_id,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    register_provider(registry, execution, source, options, provider)
}

fn register_mongodb<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    source: &'operation V7LogicalDataMigrationSource,
    credential: &'operation V7LogicalDataCredential,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<bool, String>
where
    E: CommandExecutor + V7ContainerCommandExecutor + V7ContainerRetirement + Send + Sync,
{
    let V7LogicalDataCredential::MongoDb(source_credential) = credential else {
        return Err(credential_mismatch(source, credential));
    };
    let (prepared, project) = one_target(
        options.prepared.iter().flat_map(|prepared| match prepared {
            PreparedSharedInstance::MongoDb(prepared) => prepared
                .projects()
                .iter()
                .filter(|project| {
                    matches_source(
                        source,
                        project.credential().project_id().unwrap_or_default(),
                        project.credential().service_id(),
                    )
                })
                .map(|project| (prepared, project))
                .collect(),
            _ => Vec::new(),
        }),
        source,
    )?;
    let target_container = target_container(prepared.instance().container(), options, source)?;
    let target = target_logical_resource(source, prepared.instance().container(), options)?;
    let provider = V7MongoDbMigrationProvider::new(
        options.engine,
        EngineV7MongoDbSourceRetirement::new(options.engine),
        V7MongoDbMigrationProviderOptions {
            accepted: options.accepted,
            source,
            source_credential,
            target_container,
            target_logical_resource: target,
            target_credential: project.credential(),
            target_plan: project.logical(),
            administrator: prepared.instance().bootstrap_credential(),
            installation_id: options.installation_id,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    register_provider(registry, execution, source, options, provider)
}

fn register_sql_server<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    source: &'operation V7LogicalDataMigrationSource,
    credential: &'operation V7LogicalDataCredential,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<bool, String>
where
    E: CommandExecutor + V7ContainerCommandExecutor + V7ContainerRetirement + Send + Sync,
{
    let V7LogicalDataCredential::SqlServer(source_credential) = credential else {
        return Err(credential_mismatch(source, credential));
    };
    let (prepared, project) = one_target(
        options.prepared.iter().flat_map(|prepared| match prepared {
            PreparedSharedInstance::SqlServer(prepared) => prepared
                .projects()
                .iter()
                .filter(|project| {
                    matches_source(
                        source,
                        project.credential().project_id().unwrap_or_default(),
                        project.credential().service_id(),
                    )
                })
                .map(|project| (prepared, project))
                .collect(),
            _ => Vec::new(),
        }),
        source,
    )?;
    let target_container = target_container(prepared.instance().container(), options, source)?;
    let target = target_logical_resource(source, prepared.instance().container(), options)?;
    let provider = V7SqlServerMigrationProvider::new(
        options.engine,
        EngineV7SqlServerSourceRetirement::new(options.engine),
        V7SqlServerMigrationProviderOptions {
            accepted: options.accepted,
            source,
            source_credential,
            target_container,
            target_logical_resource: target,
            target_credential: project.credential(),
            target_plan: project.logical(),
            administrator: prepared.instance().bootstrap_credential(),
            installation_id: options.installation_id,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    register_provider(registry, execution, source, options, provider)
}

fn register_redis<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    source: &'operation V7LogicalDataMigrationSource,
    credential: &'operation V7LogicalDataCredential,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<bool, String>
where
    E: CommandExecutor + V7ContainerCommandExecutor + V7ContainerRetirement + Send + Sync,
{
    let V7LogicalDataCredential::Redis(source_credential) = credential else {
        return Err(credential_mismatch(source, credential));
    };
    let (prepared, project) = one_target(
        options.prepared.iter().flat_map(|prepared| match prepared {
            PreparedSharedInstance::Redis(prepared)
                if prepared.instance().flavor().implementation() == source.driver() =>
            {
                prepared
                    .projects()
                    .iter()
                    .filter(|project| {
                        matches_source(
                            source,
                            project.credential().project_id().unwrap_or_default(),
                            project.credential().service_id(),
                        )
                    })
                    .map(|project| (prepared, project))
                    .collect()
            }
            _ => Vec::new(),
        }),
        source,
    )?;
    let target_container = target_container(prepared.instance().container(), options, source)?;
    let target = target_logical_resource(source, prepared.instance().container(), options)?;
    let provider = V7RedisMigrationProvider::new(
        options.engine,
        EngineV7RedisSourceRetirement::new(options.engine),
        V7RedisMigrationProviderOptions {
            accepted: options.accepted,
            source,
            flavor: prepared.instance().flavor(),
            source_credential,
            target_container,
            target_logical_resource: target,
            target_credential: project.credential(),
            target_acl: project.acl(),
            administrator: prepared.instance().bootstrap_credential(),
            installation_id: options.installation_id,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    register_provider(registry, execution, source, options, provider)
}

fn register_minio<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    source: &'operation V7LogicalDataMigrationSource,
    credential: &'operation V7LogicalDataCredential,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<bool, String>
where
    E: CommandExecutor + V7ContainerCommandExecutor + V7ContainerRetirement + Send + Sync,
{
    let V7LogicalDataCredential::Minio(source_credential) = credential else {
        return Err(credential_mismatch(source, credential));
    };
    let (prepared, project) = one_target(
        options.prepared.iter().flat_map(|prepared| match prepared {
            PreparedSharedInstance::ObjectStore(prepared)
                if prepared.instance().flavor() == ObjectStoreFlavor::Minio =>
            {
                prepared
                    .projects()
                    .iter()
                    .filter(|project| {
                        matches_source(
                            source,
                            project.credential().project_id().unwrap_or_default(),
                            project.credential().service_id(),
                        )
                    })
                    .map(|project| (prepared, project))
                    .collect()
            }
            _ => Vec::new(),
        }),
        source,
    )?;
    let target_container = target_container(prepared.instance().container(), options, source)?;
    let target = target_logical_resource(source, prepared.instance().container(), options)?;
    let provider = V7MinioMigrationProvider::new(
        options.engine,
        EngineV7MinioSourceRetirement::new(options.engine),
        V7MinioMigrationProviderOptions {
            accepted: options.accepted,
            source,
            source_credential,
            target_container,
            target_logical_resource: target,
            target_credential: project.credential(),
            target_definition: project.definition(),
            installation_id: options.installation_id,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    register_provider(registry, execution, source, options, provider)
}

fn register_rabbitmq<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    source: &'operation V7LogicalDataMigrationSource,
    credential: &'operation V7LogicalDataCredential,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<bool, String>
where
    E: CommandExecutor + V7ContainerCommandExecutor + V7ContainerRetirement + Send + Sync,
{
    let V7LogicalDataCredential::RabbitMq(source_credential) = credential else {
        return Err(credential_mismatch(source, credential));
    };
    let (prepared, project) = one_target(
        options.prepared.iter().flat_map(|prepared| match prepared {
            PreparedSharedInstance::RabbitMq(prepared) => prepared
                .projects()
                .iter()
                .filter(|project| {
                    matches_source(
                        source,
                        project.credential().project_id().unwrap_or_default(),
                        project.credential().service_id(),
                    )
                })
                .map(|project| (prepared, project))
                .collect(),
            _ => Vec::new(),
        }),
        source,
    )?;
    let target_container = target_container(prepared.instance().container(), options, source)?;
    let target = target_logical_resource(source, prepared.instance().container(), options)?;
    let provider = V7RabbitMqMigrationProvider::new(
        options.engine,
        EngineV7RabbitMqSourceRetirement::new(options.engine),
        V7RabbitMqMigrationProviderOptions {
            accepted: options.accepted,
            source,
            source_credential,
            target_container,
            target_logical_resource: target,
            target_credential: project.credential(),
            target_definition: project.definition(),
            installation_id: options.installation_id,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .map_err(|error| error.to_string())?;
    register_provider(registry, execution, source, options, provider)
}

fn register_provider<'operation, E, P>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    source: &'operation V7LogicalDataMigrationSource,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
    provider: P,
) -> Result<bool, String>
where
    P: crate::control_plane::migration::V7RecoverableMigrationProvider<
            V7LogicalDataMigrationSource,
        > + 'operation,
{
    register_v7_logical_data_migration_adapter(
        registry,
        execution,
        V7LogicalDataMigrationAdapterOptions {
            accepted: options.accepted,
            source,
            provider: Box::new(provider),
        },
    )
}

fn target_container<'operation, E>(
    request: &ContainerCreateOptions,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
    source: &V7LogicalDataMigrationSource,
) -> Result<&'operation OwnedContainer, String> {
    one_target(
        options
            .target_containers
            .iter()
            .filter(|container| container.metadata() == request.metadata()),
        source,
    )
}

fn target_logical_resource<'operation, E>(
    source: &V7LogicalDataMigrationSource,
    request: &ContainerCreateOptions,
    options: &RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
) -> Result<&'operation LogicalResourceRecord, String> {
    one_target(
        options.logical_resources.iter().filter(|resource| {
            resource.project_id() == source.project_id()
                && resource.service_id() == source.service_id()
                && resource.lifecycle() == ResourceLifecycle::Active
                && resource.compatibility_fingerprint()
                    == request.metadata().compatibility_fingerprint()
        }),
        source,
    )
}

fn one_target<T: Copy>(
    values: impl Iterator<Item = T>,
    source: &V7LogicalDataMigrationSource,
) -> Result<T, String> {
    let values = values.collect::<Vec<_>>();
    match values.as_slice() {
        [value] => Ok(*value),
        [] => Err(format!(
            "accepted v7 logical source '{}:{}' exact v8 target is missing",
            source.project_id(),
            source.service_id()
        )),
        _ => Err(format!(
            "accepted v7 logical source '{}:{}' exact v8 target is ambiguous",
            source.project_id(),
            source.service_id()
        )),
    }
}

fn matches_source(source: &V7LogicalDataMigrationSource, project: &str, service: &str) -> bool {
    source.project_id() == project && source.service_id() == service
}

fn credential_mismatch(
    source: &V7LogicalDataMigrationSource,
    credential: &V7LogicalDataCredential,
) -> String {
    format!(
        "accepted v7 logical source '{}:{}' driver '{}' differs from credential kind '{}'",
        source.project_id(),
        source.service_id(),
        source.driver(),
        credential.kind()
    )
}
