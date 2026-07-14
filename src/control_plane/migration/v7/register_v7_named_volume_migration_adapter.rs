use super::v7_named_volume_migration_adapter::V7NamedVolumeMigrationAdapter;
use super::{V7MigrationAdapterRegistry, V7NamedVolumeMigrationAdapterOptions};
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Registers one exact accepted legacy named-volume transition.
pub(crate) fn register_v7_named_volume_migration_adapter<'adapter>(
    registry: &mut V7MigrationAdapterRegistry<'adapter>,
    execution: &V7MigrationExecutionRecord,
    options: V7NamedVolumeMigrationAdapterOptions<'adapter>,
) -> Result<bool, String> {
    let adapter_id = format!("volume/{}", options.source.service_id());
    let Some(checkpoint) = execution
        .checkpoints()
        .iter()
        .find(|checkpoint| checkpoint.adapter_id() == adapter_id)
    else {
        return Ok(false);
    };
    if checkpoint.adapter_kind() != "named-volume-archive" {
        return Ok(false);
    }
    if !checkpoint.requires_recovery()
        || execution.project_id() != options.accepted.project_id()
        || execution.canonical_project_path() != options.accepted.canonical_project_path()
        || execution.evidence_revision() != options.accepted.evidence_revision()
    {
        return Err(
            "v7 named-volume checkpoint does not match accepted recovery evidence".to_owned(),
        );
    }
    let adapter = V7NamedVolumeMigrationAdapter::new(options)?;
    registry.register(
        checkpoint.adapter_id(),
        checkpoint.adapter_kind(),
        Box::new(adapter),
    )?;

    Ok(true)
}
