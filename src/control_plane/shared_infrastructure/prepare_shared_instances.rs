use super::{
    CredentialEntropy, MySqlPreparationOptions, ObjectStorePreparationOptions,
    PostgresPreparationOptions, PreparedSharedInstance, RedisPreparationOptions,
    SharedInstancePlan, SharedPreparationError, SharedPreparationOptions,
    prepare_mysql_shared_instances, prepare_object_store_shared_instances,
    prepare_postgres_shared_instances, prepare_redis_shared_instances,
};
use crate::control_plane::state::StateStore;

/// Dispatches exact compatibility groups to their registered preparation strategy.
pub(crate) fn prepare_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared: &[SharedInstancePlan],
    entropy: &Entropy,
    options: SharedPreparationOptions<'_>,
) -> Result<Vec<PreparedSharedInstance>, SharedPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    shared
        .iter()
        .map(|instance| match instance.profile().implementation() {
            "postgresql" => prepare_postgres_shared_instances(
                store,
                std::slice::from_ref(instance),
                entropy,
                PostgresPreparationOptions {
                    installation_id: options.installation_id,
                    network_name: options.network_name,
                    schema_version: options.schema_version,
                },
            )
            .map_err(invalid)?
            .pop()
            .map(PreparedSharedInstance::Postgres)
            .ok_or_else(|| invalid("PostgreSQL strategy returned no prepared instance")),
            "mysql" | "mariadb" => prepare_mysql_shared_instances(
                store,
                std::slice::from_ref(instance),
                entropy,
                MySqlPreparationOptions {
                    installation_id: options.installation_id,
                    network_name: options.network_name,
                    schema_version: options.schema_version,
                },
            )
            .map_err(invalid)?
            .pop()
            .map(PreparedSharedInstance::MySql)
            .ok_or_else(|| invalid("MySQL-family strategy returned no prepared instance")),
            "redis" | "valkey" => prepare_redis_shared_instances(
                store,
                std::slice::from_ref(instance),
                entropy,
                RedisPreparationOptions {
                    installation_id: options.installation_id,
                    network_name: options.network_name,
                    schema_version: options.schema_version,
                    state_directory: options.state_directory,
                },
            )
            .map_err(invalid)?
            .pop()
            .map(PreparedSharedInstance::Redis)
            .ok_or_else(|| invalid("Redis-compatible strategy returned no prepared instance")),
            "minio" => prepare_object_store_shared_instances(
                store,
                std::slice::from_ref(instance),
                entropy,
                ObjectStorePreparationOptions {
                    installation_id: options.installation_id,
                    network_name: options.network_name,
                    schema_version: options.schema_version,
                    state_directory: options.state_directory,
                },
            )
            .map_err(invalid)?
            .pop()
            .map(PreparedSharedInstance::ObjectStore)
            .ok_or_else(|| invalid("object-store strategy returned no prepared instance")),
            implementation => Err(invalid(format!(
                "shared implementation '{implementation}' has no registered preparation strategy"
            ))),
        })
        .collect()
}

fn invalid(error: impl std::fmt::Display) -> SharedPreparationError {
    SharedPreparationError::new(error.to_string())
}
