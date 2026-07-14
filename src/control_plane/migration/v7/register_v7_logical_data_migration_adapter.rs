use super::v7_logical_data_migration_adapter::V7LogicalDataMigrationAdapter;
use super::{V7LogicalDataMigrationAdapterOptions, V7MigrationAdapterRegistry};
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Registers one exact accepted logical-data transition by selected driver kind.
pub(crate) fn register_v7_logical_data_migration_adapter<'adapter>(
    registry: &mut V7MigrationAdapterRegistry<'adapter>,
    execution: &V7MigrationExecutionRecord,
    options: V7LogicalDataMigrationAdapterOptions<'adapter>,
) -> Result<bool, String> {
    let adapter_id = format!("service/{}", options.source.service_id());
    let Some(checkpoint) = execution
        .checkpoints()
        .iter()
        .find(|checkpoint| checkpoint.adapter_id() == adapter_id)
    else {
        return Ok(false);
    };
    let Some(expected_driver) = expected_driver(checkpoint.adapter_kind()) else {
        return Ok(false);
    };
    if !checkpoint.requires_recovery()
        || expected_driver != options.source.driver()
        || execution.project_id() != options.accepted.project_id()
        || execution.canonical_project_path() != options.accepted.canonical_project_path()
        || execution.evidence_revision() != options.accepted.evidence_revision()
    {
        return Err(
            "v7 logical-data checkpoint does not match accepted recovery evidence".to_owned(),
        );
    }
    let adapter = V7LogicalDataMigrationAdapter::new(options)?;
    registry.register(
        checkpoint.adapter_id(),
        checkpoint.adapter_kind(),
        Box::new(adapter),
    )?;

    Ok(true)
}

pub(super) fn expected_driver(adapter_kind: &str) -> Option<&'static str> {
    match adapter_kind {
        "mongodb-logical-database" => Some("mongodb"),
        "postgres-logical-database" => Some("postgres"),
        "mysql-logical-database" => Some("mysql"),
        "sqlserver-logical-database" => Some("sqlserver"),
        "redis-tenant-prefix" => Some("redis"),
        "valkey-tenant-prefix" => Some("valkey"),
        "minio-bucket" => Some("minio"),
        "rabbitmq-vhost" => Some("rabbitmq"),
        _ => None,
    }
}
