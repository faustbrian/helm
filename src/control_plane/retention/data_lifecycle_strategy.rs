/// Exact backup/restore adapter selected from durable logical-resource kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    /// Reports whether one recovery point necessarily covers every tenant.
    pub(crate) const fn requires_shared_instance_scope(self) -> bool {
        matches!(self, Self::SharedKeyValueSnapshot)
    }
}
