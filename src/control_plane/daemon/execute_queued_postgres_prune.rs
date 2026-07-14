use super::{PostgresPruneExecutionOptions, PostgresPruneExecutionResult};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ResourceKind, reconstruct_owned_container,
};
use crate::control_plane::retention::{
    DataLifecycleStrategy, MinioLogicalPruneOptions, MongoDbLogicalPruneOptions,
    MySqlLogicalPruneOptions, PostgresLogicalPruneOptions, PostgresLogicalPrunePlan,
    PostgresLogicalPrunePlanOptions, RabbitMqLogicalPruneOptions, RedisLogicalPruneOptions,
    SqlServerLogicalPruneOptions, prune_minio_logical_resource, prune_mongodb_logical_resource,
    prune_mysql_logical_resource, prune_postgres_logical_resource, prune_rabbitmq_logical_resource,
    prune_redis_logical_resource, prune_sql_server_logical_resource,
};
use crate::control_plane::shared_infrastructure::{MySqlFlavor, RedisFlavor};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, LogicalResourceRecord, SqliteStateStore, StateStore,
};

/// Revalidates immutable intent, deletes the tenant, then atomically retires state.
pub(crate) async fn execute_queued_postgres_prune<E>(
    engine: E,
    options: PostgresPruneExecutionOptions,
) -> PostgresPruneExecutionResult
where
    E: CommandExecutor + ContainerDiscovery,
{
    let outcome = execute(&engine, &options)
        .await
        .map_err(|error| error.to_string());

    PostgresPruneExecutionResult::new(options.operation, outcome)
}

async fn execute<E>(engine: &E, options: &PostgresPruneExecutionOptions) -> Result<(), String>
where
    E: CommandExecutor + ContainerDiscovery,
{
    if options.installation_id.is_empty()
        || options.schema_version == 0
        || options.timeout.is_zero()
    {
        return Err("logical prune runtime identity is incomplete".to_owned());
    }
    let mut store =
        SqliteStateStore::open(&options.state_database_path).map_err(|error| error.to_string())?;
    let installation = store
        .installation()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "the v8 installation identity has not been initialized".to_owned())?;
    if installation.installation_id() != options.installation_id {
        return Err("logical prune installation identity changed".to_owned());
    }
    let projects = store.projects().map_err(|error| error.to_string())?;
    let logical_resources = store
        .logical_resources()
        .map_err(|error| error.to_string())?;
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let recovery_points = store
        .recovery_points(options.operation.project_id())
        .map_err(|error| error.to_string())?;
    let plan = PostgresLogicalPrunePlan::new(PostgresLogicalPrunePlanOptions {
        installation_id: installation.installation_id(),
        project_id: options.operation.project_id(),
        service_id: options.operation.service_id(),
        recovery_point_id: options.operation.recovery_point_id(),
        project_registered: projects
            .iter()
            .any(|project| project.project_name() == options.operation.project_id()),
        logical_resources: &logical_resources,
        credentials: &credentials,
        recovery_points: &recovery_points,
    })?;
    if !options.operation.matches_plan(&plan) {
        return Err("logical prune confirmation became stale before execution".to_owned());
    }
    let logical = exact_logical(&logical_resources, &options.operation)?;
    let credential = exact_credential(&credentials, &options.operation)?;
    let administrator = exact_administrator(&credentials, &options.operation, logical)?;
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| error.to_string())?;
    let containers = observed
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
        })
        .collect::<Vec<_>>();
    let [container] = containers.as_slice() else {
        return Err(format!(
            "logical prune requires exactly one owned shared container; found {}",
            containers.len()
        ));
    };

    match options.operation.strategy() {
        DataLifecycleStrategy::PostgreSqlLogical => {
            prune_postgres_logical_resource(
                engine,
                PostgresLogicalPruneOptions {
                    installation_id: &options.installation_id,
                    container,
                    logical_resource: logical,
                    credential,
                    administrator: required_administrator(administrator)?,
                    timeout: options.timeout,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        DataLifecycleStrategy::MySqlLogical => {
            prune_mysql_logical_resource(
                engine,
                MySqlLogicalPruneOptions {
                    installation_id: &options.installation_id,
                    flavor: mysql_flavor(logical)?,
                    container,
                    logical_resource: logical,
                    credential,
                    administrator: required_administrator(administrator)?,
                    timeout: options.timeout,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        DataLifecycleStrategy::MongoDbLogical => {
            prune_mongodb_logical_resource(
                engine,
                MongoDbLogicalPruneOptions {
                    installation_id: &options.installation_id,
                    container,
                    logical_resource: logical,
                    credential,
                    administrator: required_administrator(administrator)?,
                    timeout: options.timeout,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        DataLifecycleStrategy::SqlServerNative => {
            prune_sql_server_logical_resource(
                engine,
                SqlServerLogicalPruneOptions {
                    installation_id: &options.installation_id,
                    container,
                    logical_resource: logical,
                    credential,
                    administrator: required_administrator(administrator)?,
                    timeout: options.timeout,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        DataLifecycleStrategy::RabbitMqDefinitions => {
            prune_rabbitmq_logical_resource(
                engine,
                RabbitMqLogicalPruneOptions {
                    installation_id: &options.installation_id,
                    container,
                    logical_resource: logical,
                    credential,
                    timeout: options.timeout,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        DataLifecycleStrategy::ObjectStoreBucketExport => {
            prune_minio_logical_resource(
                engine,
                MinioLogicalPruneOptions {
                    installation_id: &options.installation_id,
                    container,
                    logical_resource: logical,
                    credential,
                    administrator: required_administrator(administrator)?,
                    timeout: options.timeout,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        DataLifecycleStrategy::SharedKeyValueSnapshot => {
            prune_redis_logical_resource(
                engine,
                RedisLogicalPruneOptions {
                    installation_id: &options.installation_id,
                    flavor: redis_flavor(logical)?,
                    container,
                    logical_resource: logical,
                    credential,
                    administrator: required_administrator(administrator)?,
                    timeout: options.timeout,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        }
    }
    store
        .retire_logical_resource(logical, credential)
        .map_err(|error| error.to_string())
}

fn exact_logical<'state>(
    logical: &'state [LogicalResourceRecord],
    operation: &super::QueuedPostgresPrune,
) -> Result<&'state LogicalResourceRecord, String> {
    one(
        logical
            .iter()
            .filter(|item| item.logical_resource_id() == operation.logical_resource_id())
            .collect(),
        "orphaned logical resource",
    )
}

fn exact_credential<'state>(
    credentials: &'state [CredentialRecord],
    operation: &super::QueuedPostgresPrune,
) -> Result<&'state CredentialRecord, String> {
    one(
        credentials
            .iter()
            .filter(|item| {
                item.credential_id() == operation.credential_id()
                    && item.lifecycle() == CredentialLifecycle::Disabled
            })
            .collect(),
        "disabled tenant credential",
    )
}

fn exact_administrator<'state>(
    credentials: &'state [CredentialRecord],
    operation: &super::QueuedPostgresPrune,
    logical: &LogicalResourceRecord,
) -> Result<Option<&'state CredentialRecord>, String> {
    let fingerprint = operation
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .ok_or_else(|| "logical prune compatibility fingerprint is malformed".to_owned())?;
    let (implementation, username, suffix) = match operation.strategy() {
        DataLifecycleStrategy::PostgreSqlLogical => ("postgresql", "stackctl_admin", "bootstrap"),
        DataLifecycleStrategy::MySqlLogical => match mysql_flavor(logical)? {
            MySqlFlavor::MySql => ("mysql", "root", "bootstrap"),
            MySqlFlavor::MariaDb => ("mariadb", "root", "bootstrap"),
        },
        DataLifecycleStrategy::MongoDbLogical => ("mongodb", "stackctl_admin", "bootstrap"),
        DataLifecycleStrategy::SqlServerNative => ("sqlserver", "sa", "bootstrap"),
        DataLifecycleStrategy::ObjectStoreBucketExport => ("minio", "stackctl_admin", "root"),
        DataLifecycleStrategy::SharedKeyValueSnapshot => match redis_flavor(logical)? {
            RedisFlavor::Redis => ("redis", "stackctl_admin", "bootstrap"),
            RedisFlavor::Valkey => ("valkey", "stackctl_admin", "bootstrap"),
        },
        DataLifecycleStrategy::RabbitMqDefinitions => return Ok(None),
    };
    let administrator_id = format!("shared/{fingerprint}/{implementation}-{suffix}");
    one(
        credentials
            .iter()
            .filter(|item| {
                item.credential_id() == administrator_id
                    && item.project_id().is_none()
                    && item.service_id() == implementation
                    && item.username() == username
                    && item.lifecycle() == CredentialLifecycle::Active
            })
            .collect(),
        "active shared service administrator",
    )
    .map(Some)
}

fn required_administrator(
    administrator: Option<&CredentialRecord>,
) -> Result<&CredentialRecord, String> {
    administrator.ok_or_else(|| "logical prune strategy requires a shared administrator".to_owned())
}

fn mysql_flavor(logical: &LogicalResourceRecord) -> Result<MySqlFlavor, String> {
    match logical.kind() {
        "mysql_database" => Ok(MySqlFlavor::MySql),
        "mariadb_database" => Ok(MySqlFlavor::MariaDb),
        kind => Err(format!(
            "logical resource kind '{kind}' is not a MySQL-family tenant"
        )),
    }
}

fn redis_flavor(logical: &LogicalResourceRecord) -> Result<RedisFlavor, String> {
    match logical.kind() {
        "redis_acl_prefix" => Ok(RedisFlavor::Redis),
        "valkey_acl_prefix" => Ok(RedisFlavor::Valkey),
        kind => Err(format!(
            "logical resource kind '{kind}' is not a Redis-compatible tenant"
        )),
    }
}

fn one<'state, T>(matches: Vec<&'state T>, description: &str) -> Result<&'state T, String> {
    let [item] = matches.as_slice() else {
        return Err(format!(
            "logical prune requires exactly one {description}; found {}",
            matches.len()
        ));
    };

    Ok(*item)
}
