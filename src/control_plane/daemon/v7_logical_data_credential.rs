use crate::control_plane::migration::{
    V7MinioCredential, V7MongoDbCredential, V7MySqlCredential, V7PostgresCredential,
    V7RabbitMqCredential, V7RedisCredential, V7SqlServerCredential,
};

/// Driver-specific secret input reconstructed only from exact accepted config.
pub(crate) enum V7LogicalDataCredential {
    MongoDb(V7MongoDbCredential),
    Postgres(V7PostgresCredential),
    MySql(V7MySqlCredential),
    SqlServer(V7SqlServerCredential),
    Redis(V7RedisCredential),
    Minio(V7MinioCredential),
    RabbitMq(V7RabbitMqCredential),
}

impl std::fmt::Debug for V7LogicalDataCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MongoDb(credential) => {
                formatter.debug_tuple("MongoDb").field(credential).finish()
            }
            Self::Postgres(credential) => {
                formatter.debug_tuple("Postgres").field(credential).finish()
            }
            Self::MySql(credential) => formatter.debug_tuple("MySql").field(credential).finish(),
            Self::SqlServer(credential) => formatter
                .debug_tuple("SqlServer")
                .field(credential)
                .finish(),
            Self::Redis(credential) => formatter.debug_tuple("Redis").field(credential).finish(),
            Self::Minio(credential) => formatter.debug_tuple("Minio").field(credential).finish(),
            Self::RabbitMq(credential) => {
                formatter.debug_tuple("RabbitMq").field(credential).finish()
            }
        }
    }
}

impl V7LogicalDataCredential {
    pub(crate) const fn kind(&self) -> &'static str {
        match self {
            Self::MongoDb(_) => "mongodb",
            Self::Postgres(_) => "postgres",
            Self::MySql(_) => "mysql",
            Self::SqlServer(_) => "sqlserver",
            Self::Redis(_) => "redis",
            Self::Minio(_) => "minio",
            Self::RabbitMq(_) => "rabbitmq",
        }
    }
}
