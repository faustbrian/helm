use super::{ProjectBackupExecutionOptions, ProjectBackupExecutionResult};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, ContainerVolumeArchive, ResourceKind,
    VolumeDiscovery, reconstruct_owned_container, reconstruct_owned_volume,
};
use crate::control_plane::migration::{
    MinioBackupOptions, MongoDbBackupOptions, MySqlBackupOptions, PostgresBackupOptions,
    ProjectVolumeBackupOptions, RabbitMqBackupOptions, RedisBackupOptions, SqlServerBackupOptions,
    backup_minio_bucket, backup_mongodb_database, backup_mysql_database, backup_postgres_database,
    backup_project_volume, backup_rabbitmq_vhost, backup_redis_prefix, backup_sql_server_database,
};
use crate::control_plane::shared_infrastructure::{MySqlFlavor, RedisFlavor};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};

/// Resolves exact live ownership and creates one verified recovery point.
pub(crate) async fn execute_queued_project_backup<E>(
    mut engine: E,
    options: ProjectBackupExecutionOptions,
) -> ProjectBackupExecutionResult
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + ContainerVolumeArchive
        + VolumeDiscovery,
{
    let created_at_unix_seconds = options.created_at_unix_seconds;
    let outcome = if options.operation.kind() == "volume" {
        Box::pin(execute_project_volume_backup(&mut engine, &options)).await
    } else {
        Box::pin(execute_logical_project_backup(&engine, &options)).await
    };

    ProjectBackupExecutionResult::new(options.operation, created_at_unix_seconds, outcome)
}

async fn execute_logical_project_backup<E>(
    engine: &E,
    options: &ProjectBackupExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationBackup, String>
where
    E: CommandExecutor + ContainerDiscovery,
{
    let logical = options
        .logical_resource
        .as_ref()
        .map_err(ToString::to_string)?;
    let credential = options.credential.as_ref().map_err(ToString::to_string)?;
    validate_runtime_state(options, logical, credential)?;
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| error.to_string())?;
    let mut matches = observed
        .iter()
        .filter_map(|container| {
            reconstruct_owned_container(container, &options.installation_id, options.schema_version)
                .ok()
        })
        .filter(|container| {
            container.metadata().kind() == ResourceKind::SharedService
                && container.metadata().project_id().is_none()
                && container.metadata().compatibility_fingerprint()
                    == options.operation.compatibility_fingerprint()
        });
    let container = matches.next().ok_or_else(|| {
        format!(
            "project backup found no owned shared service for compatibility '{}'",
            options.operation.compatibility_fingerprint()
        )
    })?;
    if matches.next().is_some() {
        return Err(format!(
            "project backup found multiple owned shared services for compatibility '{}'",
            options.operation.compatibility_fingerprint()
        ));
    }
    let redis_prefix = format!(
        "stackctl:{}:{}:",
        logical.project_id(),
        logical.service_id()
    );
    match logical.kind() {
        "postgres_database_and_role" => backup_postgres_database(
            engine,
            &container,
            &PostgresBackupOptions {
                logical_resource: logical,
                credential,
                database_name: logical.logical_resource_id(),
                installation_id: &options.installation_id,
                created_at_unix_seconds: options.created_at_unix_seconds,
                backup_root: &options.backup_root,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string()),
        "mysql_database" | "mariadb_database" => backup_mysql_database(
            engine,
            &container,
            &MySqlBackupOptions {
                flavor: mysql_flavor(logical.kind())?,
                logical_resource: logical,
                credential,
                database_name: logical.logical_resource_id(),
                installation_id: &options.installation_id,
                created_at_unix_seconds: options.created_at_unix_seconds,
                backup_root: &options.backup_root,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string()),
        "mongodb_database" => backup_mongodb_database(
            engine,
            &container,
            &MongoDbBackupOptions {
                logical_resource: logical,
                credential,
                database_name: logical.logical_resource_id(),
                installation_id: &options.installation_id,
                created_at_unix_seconds: options.created_at_unix_seconds,
                backup_root: &options.backup_root,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string()),
        "sqlserver_database" => backup_sql_server_database(
            engine,
            &container,
            &SqlServerBackupOptions {
                logical_resource: logical,
                credential,
                database_name: logical.logical_resource_id(),
                installation_id: &options.installation_id,
                created_at_unix_seconds: options.created_at_unix_seconds,
                backup_root: &options.backup_root,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string()),
        "rabbitmq_vhost_user" => backup_rabbitmq_vhost(
            engine,
            &container,
            &RabbitMqBackupOptions {
                logical_resource: logical,
                credential,
                installation_id: &options.installation_id,
                created_at_unix_seconds: options.created_at_unix_seconds,
                backup_root: &options.backup_root,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string()),
        "minio_bucket_policy" => backup_minio_bucket(
            engine,
            &container,
            &MinioBackupOptions {
                logical_resource: logical,
                credential,
                installation_id: &options.installation_id,
                created_at_unix_seconds: options.created_at_unix_seconds,
                backup_root: &options.backup_root,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string()),
        "redis_acl_prefix" | "valkey_acl_prefix" => backup_redis_prefix(
            engine,
            &container,
            &RedisBackupOptions {
                flavor: redis_flavor(logical.kind())?,
                logical_resource: logical,
                credential,
                administrator: options
                    .administrator
                    .as_ref()
                    .map_err(ToString::to_string)?
                    .as_ref()
                    .ok_or_else(|| {
                        "Redis-compatible backup requires a shared administrator".to_owned()
                    })?,
                prefix: &redis_prefix,
                installation_id: &options.installation_id,
                created_at_unix_seconds: options.created_at_unix_seconds,
                backup_root: &options.backup_root,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string()),
        kind => Err(format!("project backup kind '{kind}' is not implemented")),
    }
}

async fn execute_project_volume_backup<E>(
    engine: &mut E,
    options: &ProjectBackupExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationBackup, String>
where
    E: ContainerDiscovery + ContainerLifecycle + ContainerVolumeArchive + VolumeDiscovery,
{
    let resource = options
        .physical_resource
        .as_ref()
        .map_err(ToString::to_string)?
        .as_ref()
        .ok_or_else(|| "project volume backup has no exact physical resource".to_owned())?;
    let containers = engine
        .discover_managed()
        .await
        .map_err(|error| error.to_string())?;
    let mut container_matches = containers
        .iter()
        .filter_map(|container| {
            reconstruct_owned_container(container, &options.installation_id, options.schema_version)
                .ok()
        })
        .filter(|container| {
            container.metadata().kind() == ResourceKind::ProjectService
                && container.metadata().project_id() == Some(options.operation.project_id())
                && container.metadata().resource_id() == Some(options.operation.service_id())
                && container.metadata().compatibility_fingerprint()
                    == options.operation.compatibility_fingerprint()
        });
    let container = container_matches.next().ok_or_else(|| {
        format!(
            "project volume backup found no exact owned service '{}-{}'",
            options.operation.project_id(),
            options.operation.service_id()
        )
    })?;
    if container_matches.next().is_some() {
        return Err(format!(
            "project volume backup found multiple owned services '{}-{}'",
            options.operation.project_id(),
            options.operation.service_id()
        ));
    }
    let volumes = engine
        .discover_managed_volumes()
        .await
        .map_err(|error| error.to_string())?;
    let mut volume_matches = volumes
        .iter()
        .filter_map(|volume| {
            reconstruct_owned_volume(volume, &options.installation_id, options.schema_version).ok()
        })
        .filter(|volume| {
            volume.name() == resource.resource_id()
                && volume.metadata().kind() == ResourceKind::Volume
                && volume.metadata().project_id() == Some(options.operation.project_id())
                && volume.metadata().resource_id() == Some(options.operation.service_id())
                && volume.metadata().compatibility_fingerprint()
                    == options.operation.compatibility_fingerprint()
        });
    let volume = volume_matches.next().ok_or_else(|| {
        format!(
            "project volume backup found no exact owned volume '{}'",
            resource.resource_id()
        )
    })?;
    if volume_matches.next().is_some() {
        return Err(format!(
            "project volume backup found multiple owned volumes '{}'",
            resource.resource_id()
        ));
    }

    Box::pin(backup_project_volume(
        engine,
        &container,
        &volume,
        &ProjectVolumeBackupOptions {
            resource,
            installation_id: &options.installation_id,
            project_id: options.operation.project_id(),
            service_id: options.operation.service_id(),
            created_at_unix_seconds: options.created_at_unix_seconds,
            backup_root: &options.backup_root,
            timeout: options.timeout,
        },
    ))
    .await
    .map_err(|error| error.to_string())
}

fn redis_flavor(kind: &str) -> Result<RedisFlavor, String> {
    match kind {
        "redis_acl_prefix" => Ok(RedisFlavor::Redis),
        "valkey_acl_prefix" => Ok(RedisFlavor::Valkey),
        _ => Err(format!(
            "logical resource kind '{kind}' is not Redis-compatible"
        )),
    }
}

fn mysql_flavor(kind: &str) -> Result<MySqlFlavor, String> {
    match kind {
        "mysql_database" => Ok(MySqlFlavor::MySql),
        "mariadb_database" => Ok(MySqlFlavor::MariaDb),
        _ => Err(format!(
            "logical resource kind '{kind}' is not MySQL-family"
        )),
    }
}

fn validate_runtime_state(
    options: &ProjectBackupExecutionOptions,
    logical: &crate::control_plane::state::LogicalResourceRecord,
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), String> {
    let operation = &options.operation;
    let valid = logical.logical_resource_id() == operation.logical_resource_id()
        && logical.project_id() == operation.project_id()
        && logical.service_id() == operation.service_id()
        && logical.kind() == operation.kind()
        && logical.compatibility_fingerprint() == operation.compatibility_fingerprint()
        && logical.lifecycle() == ResourceLifecycle::Active
        && credential.project_id() == Some(operation.project_id())
        && credential.service_id() == operation.service_id()
        && credential.lifecycle() == CredentialLifecycle::Active
        && options.created_at_unix_seconds >= 0
        && options.schema_version > 0
        && !options.installation_id.is_empty();
    if !valid {
        return Err(
            "project backup intent does not match active logical ownership and credentials"
                .to_owned(),
        );
    }

    Ok(())
}
