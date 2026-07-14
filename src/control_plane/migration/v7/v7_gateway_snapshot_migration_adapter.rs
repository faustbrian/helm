use super::{
    V7GatewaySnapshotMigrationAdapterOptions, V7MigrationAdapterExecutor, V7MigrationAdapterTarget,
};
use crate::control_plane::gateway::{
    GatewayConfiguration, GatewaySnapshot, reconcile_gateway_configuration,
};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, store_backup_artifact_for_identity,
    verify_stored_backup_artifact,
};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;
use std::path::Path;

/// Replaces complete gateway snapshots with verified rollback evidence.
pub(super) struct V7GatewaySnapshotMigrationAdapter<'operation> {
    provider: &'operation mut dyn GatewayConfiguration,
    rollback_snapshot: &'operation GatewaySnapshot,
    target_snapshot: &'operation GatewaySnapshot,
    identity: BackupResourceIdentity,
    backup_root: &'operation Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    rollback_bytes: Vec<u8>,
}

impl<'operation> V7GatewaySnapshotMigrationAdapter<'operation> {
    pub(super) fn new(
        project_id: &str,
        evidence_revision: &str,
        options: V7GatewaySnapshotMigrationAdapterOptions<'operation>,
    ) -> Result<Self, String> {
        if project_id.is_empty()
            || evidence_revision.is_empty()
            || !options.backup_root.is_absolute()
            || options.created_at_unix_seconds < 0
            || options.verified_at_unix_seconds < options.created_at_unix_seconds
        {
            return Err(
                "v7 gateway adapter requires immutable identity, an absolute backup root, and non-regressing times"
                    .to_owned(),
            );
        }
        let rollback_bytes = snapshot_bytes(options.rollback_snapshot)?;

        Ok(Self {
            provider: options.provider,
            rollback_snapshot: options.rollback_snapshot,
            target_snapshot: options.target_snapshot,
            identity: BackupResourceIdentity::for_v7_gateway_snapshot(
                project_id,
                evidence_revision,
            ),
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            rollback_bytes,
        })
    }

    fn verify_recovery(
        &self,
        checkpoint: &V7MigrationAdapterCheckpoint,
    ) -> Result<(), MigrationOperationError> {
        let reference = checkpoint.recovery_reference().ok_or_else(|| {
            MigrationOperationError::new("gateway rollback checkpoint has no recovery reference")
        })?;
        let stored = open_stored_backup_artifact(reference)
            .map_err(|error| operation_error("open gateway rollback", error))?;
        let verified = verify_stored_backup_artifact(&stored, self.verified_at_unix_seconds)
            .map_err(|error| operation_error("verify gateway rollback", error))?;
        let artifact = std::fs::read(stored.artifact_file())
            .map_err(|error| operation_error("read gateway rollback", error))?;
        if !verified.matches_identity(&self.identity)
            || checkpoint.recovery_artifact_sha256() != Some(verified.artifact_sha256())
            || checkpoint.recovery_artifact_size_bytes() != Some(verified.artifact_size_bytes())
            || artifact != self.rollback_bytes
        {
            return Err(MigrationOperationError::new(
                "gateway rollback artifact does not match the prepared snapshot",
            ));
        }

        Ok(())
    }
}

impl V7MigrationAdapterExecutor for V7GatewaySnapshotMigrationAdapter<'_> {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        Box::pin(async move {
            let stored = store_backup_artifact_for_identity(
                &self.identity,
                &self.rollback_bytes,
                self.created_at_unix_seconds,
                self.backup_root,
            )
            .map_err(|error| operation_error("store gateway rollback", error))?;
            let verified = verify_stored_backup_artifact(&stored, self.verified_at_unix_seconds)
                .map_err(|error| operation_error("verify gateway rollback", error))?;
            let reference = stored.recovery_point().to_str().ok_or_else(|| {
                MigrationOperationError::new("gateway rollback path is not valid UTF-8")
            })?;
            MigrationBackup::new(
                reference,
                verified.artifact_sha256(),
                verified.artifact_size_bytes(),
            )
        })
    }

    fn prepare_target<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        let revision = self.target_snapshot.revision().to_owned();
        Box::pin(async move {
            V7MigrationAdapterTarget::resource(revision).map_err(MigrationOperationError::new)
        })
    }

    fn cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            if checkpoint.target_reference() != Some(self.target_snapshot.revision()) {
                return Err(MigrationOperationError::new(
                    "gateway target revision differs from the prepared snapshot",
                ));
            }
            reconcile_gateway_configuration(self.provider, self.target_snapshot)
                .await
                .map(|_| ())
                .map_err(|error| operation_error("apply gateway target", error))
        })
    }

    fn rollback<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            self.verify_recovery(checkpoint)?;
            reconcile_gateway_configuration(self.provider, self.rollback_snapshot)
                .await
                .map(|_| ())
                .map_err(|error| operation_error("apply gateway rollback", error))
        })
    }

    fn confirm<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }
}

fn snapshot_bytes(snapshot: &GatewaySnapshot) -> Result<Vec<u8>, String> {
    serde_json::to_vec(
        &snapshot
            .routes()
            .iter()
            .map(|route| (route.domain(), route.upstream()))
            .collect::<Vec<_>>(),
    )
    .map_err(|error| format!("failed to encode gateway rollback snapshot: {error}"))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
