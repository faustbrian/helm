use super::{
    V7MigrationAdapterExecutor, V7MigrationAdapterTarget,
    V7ProtectedGeneratedEnvironmentAdapterOptions, read_v7_generated_environment_rollback,
};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture, MigrationOperationError};
use crate::control_plane::state::{EnvironmentLifecycle, V7MigrationAdapterCheckpoint};

/// Verifies accepted `.env` recovery while v8 publishes managed state globally.
pub(crate) struct V7ProtectedGeneratedEnvironmentAdapter {
    project_id: String,
    evidence_revision: String,
    recovery_reference: String,
    recovery_artifact_sha256: String,
    recovery_artifact_size_bytes: u64,
    recovery_material: super::V7GeneratedEnvironmentRollbackMaterial,
    target: V7MigrationAdapterTarget,
    verified_at_unix_seconds: i64,
    maximum_environment_bytes: usize,
}

impl V7ProtectedGeneratedEnvironmentAdapter {
    pub(crate) fn new(
        options: V7ProtectedGeneratedEnvironmentAdapterOptions<'_>,
    ) -> Result<Self, String> {
        let rollback = options
            .accepted
            .generated_environment_rollback()
            .ok_or_else(|| {
                "accepted v7 environment has no protected rollback evidence".to_owned()
            })?;
        let recovery_reference = rollback.reference().to_str().ok_or_else(|| {
            "protected v7 environment rollback path is not valid UTF-8".to_owned()
        })?;
        if options.accepted.project_id() != options.target_environment.project_id()
            || options.target_environment.lifecycle() != EnvironmentLifecycle::Active
            || options.target_environment.revision().is_empty()
            || options.verified_at_unix_seconds < options.accepted.accepted_at_unix_seconds()
            || options.maximum_environment_bytes == 0
        {
            return Err(
                "protected v7 environment adapter requires matching accepted identity, an active revision, non-regressing verification time, and a positive byte bound"
                    .to_owned(),
            );
        }
        let target = V7MigrationAdapterTarget::resource(format!(
            "managed-environment:{}:{}",
            options.target_environment.project_id(),
            options.target_environment.revision()
        ))?;

        Ok(Self {
            project_id: options.accepted.project_id().to_owned(),
            evidence_revision: options.accepted.evidence_revision().to_owned(),
            recovery_reference: recovery_reference.to_owned(),
            recovery_artifact_sha256: rollback.artifact_sha256().to_owned(),
            recovery_artifact_size_bytes: rollback.artifact_size_bytes(),
            recovery_material: super::V7GeneratedEnvironmentRollbackMaterial::new(
                rollback.reference().to_path_buf(),
                rollback.artifact_sha256().to_owned(),
                rollback.artifact_size_bytes(),
            ),
            target,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            maximum_environment_bytes: options.maximum_environment_bytes,
        })
    }
}

impl V7MigrationAdapterExecutor for V7ProtectedGeneratedEnvironmentAdapter {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        Box::pin(async move {
            read_v7_generated_environment_rollback(
                &self.recovery_material,
                &self.project_id,
                &self.evidence_revision,
                self.verified_at_unix_seconds,
                self.maximum_environment_bytes,
            )
            .map_err(|error| MigrationOperationError::new(error.to_string()))?;
            MigrationBackup::new(
                self.recovery_reference.clone(),
                self.recovery_artifact_sha256.clone(),
                self.recovery_artifact_size_bytes,
            )
        })
    }

    fn prepare_target<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        let target = self.target.clone();
        Box::pin(async move { Ok(target) })
    }

    fn cutover<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn rollback<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn confirm<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        // The project `.env` is user-owned. Global state transactions switch
        // managed injection; confirmation must not rewrite or delete this file.
        Box::pin(async { Ok(()) })
    }
}
