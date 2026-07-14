use super::{V7MigrationAdapterRegistry, V7NoOpMigrationAdapter};
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Registers every explicitly selected adapter that owns no external work.
pub(crate) fn register_v7_no_op_migration_adapters(
    registry: &mut V7MigrationAdapterRegistry,
    execution: &V7MigrationExecutionRecord,
) -> Result<usize, String> {
    let mut registered = 0;
    for checkpoint in execution
        .checkpoints()
        .iter()
        .filter(|checkpoint| is_no_op_kind(checkpoint.adapter_kind()))
    {
        if checkpoint.requires_recovery() {
            return Err(format!(
                "no-op v7 adapter '{}' cannot satisfy required recovery",
                checkpoint.adapter_id()
            ));
        }
        registry.register(
            checkpoint.adapter_id(),
            checkpoint.adapter_kind(),
            Box::new(V7NoOpMigrationAdapter),
        )?;
        registered += 1;
    }

    Ok(registered)
}

fn is_no_op_kind(adapter_kind: &str) -> bool {
    matches!(
        adapter_kind,
        "no-named-volumes"
            | "logical-data-owns-storage"
            | "no-routes"
            | "no-legacy-trust-transition"
            | "no-generated-environment"
    )
}
