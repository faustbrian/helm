use super::v7_installation_trust_migration_adapter::V7InstallationTrustMigrationAdapter;
use super::{V7InstallationTrustMigrationAdapterOptions, V7MigrationAdapterRegistry};
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Registers the accepted installation trust transition when it was selected.
pub(crate) fn register_v7_installation_trust_migration_adapter<'adapter>(
    registry: &mut V7MigrationAdapterRegistry<'adapter>,
    execution: &V7MigrationExecutionRecord,
    options: V7InstallationTrustMigrationAdapterOptions<'adapter>,
) -> Result<bool, String> {
    let Some(checkpoint) = execution
        .checkpoints()
        .iter()
        .find(|checkpoint| checkpoint.adapter_id() == "trust")
    else {
        return Ok(false);
    };
    if checkpoint.adapter_kind() != "installation-legacy-caddy-ca-transition" {
        return Ok(false);
    }
    if !checkpoint.requires_recovery()
        || execution.project_id() != options.accepted.project_id()
        || execution.canonical_project_path() != options.accepted.canonical_project_path()
        || execution.evidence_revision() != options.accepted.evidence_revision()
    {
        return Err(
            "v7 trust checkpoint does not match accepted installation recovery evidence".to_owned(),
        );
    }
    let adapter = V7InstallationTrustMigrationAdapter::new(
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
