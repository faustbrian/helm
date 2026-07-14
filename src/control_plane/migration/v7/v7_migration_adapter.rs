/// Resource-specific operation selected before any v7 migration mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum V7MigrationServiceAdapter {
    MongoDbLogicalDatabase,
    PostgresLogicalDatabase,
    MySqlLogicalDatabase,
    SqlServerLogicalDatabase,
    RedisTenantPrefix,
    ValkeyTenantPrefix,
    MinioBucket,
    RabbitMqVhost,
    RecreateProjectWorkload,
    RecreateStateless,
    RecreateEphemeral,
}

impl V7MigrationServiceAdapter {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::MongoDbLogicalDatabase => "mongodb-logical-database",
            Self::PostgresLogicalDatabase => "postgres-logical-database",
            Self::MySqlLogicalDatabase => "mysql-logical-database",
            Self::SqlServerLogicalDatabase => "sqlserver-logical-database",
            Self::RedisTenantPrefix => "redis-tenant-prefix",
            Self::ValkeyTenantPrefix => "valkey-tenant-prefix",
            Self::MinioBucket => "minio-bucket",
            Self::RabbitMqVhost => "rabbitmq-vhost",
            Self::RecreateProjectWorkload => "recreate-project-workload",
            Self::RecreateStateless => "recreate-stateless",
            Self::RecreateEphemeral => "recreate-ephemeral",
        }
    }
}

/// Storage transition selected independently from service provisioning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum V7VolumeMigrationAdapter {
    NoNamedVolumes,
    LogicalDataOwnsStorage,
    NamedVolumeArchive,
}

impl V7VolumeMigrationAdapter {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::NoNamedVolumes => "no-named-volumes",
            Self::LogicalDataOwnsStorage => "logical-data-owns-storage",
            Self::NamedVolumeArchive => "named-volume-archive",
        }
    }
}

/// Route transition selected for an accepted v7 route set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum V7RouteMigrationAdapter {
    NoRoutes,
    GatewaySnapshotCutover,
}

impl V7RouteMigrationAdapter {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::NoRoutes => "no-routes",
            Self::GatewaySnapshotCutover => "gateway-snapshot-cutover",
        }
    }
}

/// Host trust transition selected for accepted public CA evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum V7TrustMigrationAdapter {
    NoLegacyTrustTransition,
    InstallationLegacyCaddyCaTransition,
}

impl V7TrustMigrationAdapter {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::NoLegacyTrustTransition => "no-legacy-trust-transition",
            Self::InstallationLegacyCaddyCaTransition => "installation-legacy-caddy-ca-transition",
        }
    }
}

/// Generated-environment transition selected before source modification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum V7EnvironmentMigrationAdapter {
    NoGeneratedEnvironment,
    ProtectedGeneratedEnvironment,
}

impl V7EnvironmentMigrationAdapter {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::NoGeneratedEnvironment => "no-generated-environment",
            Self::ProtectedGeneratedEnvironment => "protected-generated-environment",
        }
    }
}
