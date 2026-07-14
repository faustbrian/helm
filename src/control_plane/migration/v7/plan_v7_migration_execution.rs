use super::{
    V7EnvironmentMigrationAdapter, V7MigrationExecutionPlanOptions, V7MigrationServiceAdapter,
    V7RouteMigrationAdapter, V7TrustMigrationAdapter, V7VolumeMigrationAdapter,
};
use crate::control_plane::state::{
    V7MigrationAdapterCheckpoint, V7MigrationExecutionPhase, V7MigrationExecutionRecord,
    V7MigrationExecutionRecordOptions,
};

/// Creates the immutable pending checkpoint set for one selected adapter plan.
pub(crate) fn plan_v7_migration_execution(
    options: V7MigrationExecutionPlanOptions<'_>,
) -> Result<V7MigrationExecutionRecord, String> {
    let mut checkpoints = Vec::new();
    for service in options.adapter_plan.services() {
        checkpoints.push(V7MigrationAdapterCheckpoint::pending(
            format!("service/{}", service.service_id()),
            service.adapter().label(),
            service_requires_recovery(*service.adapter()),
            options.planned_at_unix_seconds,
        )?);
        checkpoints.push(V7MigrationAdapterCheckpoint::pending(
            format!("volume/{}", service.service_id()),
            service.volume_adapter().label(),
            service.volume_adapter() == V7VolumeMigrationAdapter::NamedVolumeArchive,
            options.planned_at_unix_seconds,
        )?);
    }
    checkpoints.push(V7MigrationAdapterCheckpoint::pending(
        "route",
        options.adapter_plan.route_adapter().label(),
        options.adapter_plan.route_adapter() == V7RouteMigrationAdapter::GatewaySnapshotCutover,
        options.planned_at_unix_seconds,
    )?);
    checkpoints.push(V7MigrationAdapterCheckpoint::pending(
        "trust",
        options.adapter_plan.trust_adapter().label(),
        options.adapter_plan.trust_adapter()
            == V7TrustMigrationAdapter::InstallationLegacyCaddyCaTransition,
        options.planned_at_unix_seconds,
    )?);
    checkpoints.push(V7MigrationAdapterCheckpoint::pending(
        "environment",
        options.adapter_plan.environment_adapter().label(),
        options.adapter_plan.environment_adapter()
            == V7EnvironmentMigrationAdapter::ProtectedGeneratedEnvironment,
        options.planned_at_unix_seconds,
    )?);

    V7MigrationExecutionRecord::new(V7MigrationExecutionRecordOptions {
        project_id: options.project_id.to_owned(),
        canonical_project_path: options.canonical_project_path.to_path_buf(),
        evidence_revision: options.adapter_plan.evidence_revision().to_owned(),
        adapter_plan_revision: options.adapter_plan.plan_revision().to_owned(),
        phase: V7MigrationExecutionPhase::Planned,
        checkpoints,
        updated_at_unix_seconds: options.planned_at_unix_seconds,
    })
}

const fn service_requires_recovery(adapter: V7MigrationServiceAdapter) -> bool {
    matches!(
        adapter,
        V7MigrationServiceAdapter::MongoDbLogicalDatabase
            | V7MigrationServiceAdapter::PostgresLogicalDatabase
            | V7MigrationServiceAdapter::MySqlLogicalDatabase
            | V7MigrationServiceAdapter::SqlServerLogicalDatabase
            | V7MigrationServiceAdapter::RedisTenantPrefix
            | V7MigrationServiceAdapter::ValkeyTenantPrefix
            | V7MigrationServiceAdapter::MinioBucket
            | V7MigrationServiceAdapter::RabbitMqVhost
    )
}
