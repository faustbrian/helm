use serde::{Deserialize, Serialize};

/// Exact backup/restore adapter selected from durable logical-resource kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DataLifecycleStrategy {
    PostgreSqlLogical,
    MySqlLogical,
    MongoDbLogical,
    SqlServerNative,
    RabbitMqDefinitions,
    ObjectStoreBucketExport,
    SharedKeyValueSnapshot,
}

impl DataLifecycleStrategy {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::PostgreSqlLogical => "postgresql_logical",
            Self::MySqlLogical => "mysql_logical",
            Self::MongoDbLogical => "mongodb_logical",
            Self::SqlServerNative => "sql_server_native",
            Self::RabbitMqDefinitions => "rabbitmq_definitions",
            Self::ObjectStoreBucketExport => "object_store_bucket_export",
            Self::SharedKeyValueSnapshot => "shared_key_value_snapshot",
        }
    }

    /// Reports whether one recovery point necessarily covers every tenant.
    pub(crate) const fn requires_shared_instance_scope(self) -> bool {
        matches!(self, Self::SharedKeyValueSnapshot)
    }
}
