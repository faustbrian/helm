use super::v7_gateway_snapshot_migration_adapter::V7GatewaySnapshotMigrationAdapter;
use super::{V7GatewaySnapshotMigrationAdapterOptions, V7MigrationAdapterRegistry};
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Registers the complete gateway snapshot transition when routes were selected.
pub(crate) fn register_v7_gateway_snapshot_migration_adapter<'adapter>(
    registry: &mut V7MigrationAdapterRegistry<'adapter>,
    execution: &V7MigrationExecutionRecord,
    options: V7GatewaySnapshotMigrationAdapterOptions<'adapter>,
) -> Result<bool, String> {
    let Some(checkpoint) = execution
        .checkpoints()
        .iter()
        .find(|checkpoint| checkpoint.adapter_id() == "route")
    else {
        return Ok(false);
    };
    if checkpoint.adapter_kind() != "gateway-snapshot-cutover" {
        return Ok(false);
    }
    if !checkpoint.requires_recovery() {
        return Err("v7 gateway snapshot checkpoint must require recovery".to_owned());
    }
    let adapter = V7GatewaySnapshotMigrationAdapter::new(
        execution.project_id(),
        execution.evidence_revision(),
        options,
    )?;
    registry.register(
        checkpoint.adapter_id(),
        checkpoint.adapter_kind(),
        Box::new(adapter),
    )?;

    Ok(true)
}
