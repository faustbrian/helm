use crate::control_plane::migration::{
    V7GeneratedEnvironmentRollbackMaterial, read_v7_generated_environment_rollback,
};
use crate::control_plane::state::AcceptedV7InventoryRecord;

/// Revalidates protected exact environment bytes before accepted evidence reuse.
pub(crate) fn verify_accepted_v7_environment_rollback(
    accepted: &AcceptedV7InventoryRecord,
    verified_at_unix_seconds: i64,
    maximum_environment_bytes: usize,
) -> Result<(), String> {
    let Some(rollback) = accepted.generated_environment_rollback() else {
        return if accepted.requires_generated_environment_rollback() {
            Err(
                "accepted legacy inventory lacks protected generated-environment rollback"
                    .to_owned(),
            )
        } else {
            Ok(())
        };
    };
    let material = V7GeneratedEnvironmentRollbackMaterial::new(
        rollback.reference().to_path_buf(),
        rollback.artifact_sha256().to_owned(),
        rollback.artifact_size_bytes(),
    );
    read_v7_generated_environment_rollback(
        &material,
        accepted.project_id(),
        accepted.evidence_revision(),
        verified_at_unix_seconds,
        maximum_environment_bytes,
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}
