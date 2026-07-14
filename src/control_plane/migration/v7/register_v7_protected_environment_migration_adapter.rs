use super::{
    V7MigrationAdapterRegistry, V7ProtectedGeneratedEnvironmentAdapter,
    V7ProtectedGeneratedEnvironmentAdapterOptions,
};
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Registers the exact accepted generated-environment transition when selected.
pub(crate) fn register_v7_protected_environment_migration_adapter(
    registry: &mut V7MigrationAdapterRegistry<'_>,
    execution: &V7MigrationExecutionRecord,
    options: V7ProtectedGeneratedEnvironmentAdapterOptions<'_>,
) -> Result<bool, String> {
    let Some(checkpoint) = execution
        .checkpoints()
        .iter()
        .find(|checkpoint| checkpoint.adapter_id() == "environment")
    else {
        return Ok(false);
    };
    if checkpoint.adapter_kind() != "protected-generated-environment" {
        return Ok(false);
    }
    if !checkpoint.requires_recovery()
        || execution.project_id() != options.accepted.project_id()
        || execution.canonical_project_path() != options.accepted.canonical_project_path()
        || execution.evidence_revision() != options.accepted.evidence_revision()
    {
        return Err(
            "protected v7 environment checkpoint does not match accepted recovery evidence"
                .to_owned(),
        );
    }
    let adapter = V7ProtectedGeneratedEnvironmentAdapter::new(options)?;
    registry.register(
        checkpoint.adapter_id(),
        checkpoint.adapter_kind(),
        Box::new(adapter),
    )?;

    Ok(true)
}
