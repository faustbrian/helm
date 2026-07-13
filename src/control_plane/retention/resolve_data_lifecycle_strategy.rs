use super::{DataLifecycleStrategy, DataLifecycleStrategyError};
use crate::control_plane::state::LogicalResourceRecord;

/// Selects one explicit lifecycle adapter without preset-name guessing.
pub(crate) fn resolve_data_lifecycle_strategy(
    resource: &LogicalResourceRecord,
) -> Result<DataLifecycleStrategy, DataLifecycleStrategyError> {
    match resource.kind() {
        "postgres_database_and_role" => Ok(DataLifecycleStrategy::PostgreSqlLogical),
        "mysql_database" | "mariadb_database" => Ok(DataLifecycleStrategy::MySqlLogical),
        "mongodb_database" => Ok(DataLifecycleStrategy::MongoDbLogical),
        "sqlserver_database" => Ok(DataLifecycleStrategy::SqlServerNative),
        "rabbitmq_vhost_user" => Ok(DataLifecycleStrategy::RabbitMqDefinitions),
        "minio_bucket_policy" | "garage_bucket_policy" => {
            Ok(DataLifecycleStrategy::ObjectStoreBucketExport)
        }
        "redis_acl_prefix" | "valkey_acl_prefix" | "dragonfly_acl_prefix" => {
            Ok(DataLifecycleStrategy::SharedKeyValueSnapshot)
        }
        "mailpit_smtp_identity" | "gotenberg_endpoint" => {
            Err(DataLifecycleStrategyError::NonAuthoritative {
                kind: resource.kind().to_owned(),
            })
        }
        kind => Err(DataLifecycleStrategyError::UnknownKind {
            kind: kind.to_owned(),
        }),
    }
}
