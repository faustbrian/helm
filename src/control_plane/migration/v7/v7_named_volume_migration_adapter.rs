use super::{
    V7MigrationAdapterExecutor, V7MigrationAdapterTarget, V7NamedVolumeMigrationAdapterOptions,
    V7NamedVolumeMigrationMount, V7NamedVolumeMigrationSource, V7RecoverableMigrationProvider,
};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture, MigrationOperationError};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;

/// Archives accepted legacy volumes and binds their restored v8 target.
pub(super) struct V7NamedVolumeMigrationAdapter<'operation> {
    source: V7NamedVolumeMigrationSource,
    provider: Box<dyn V7RecoverableMigrationProvider<V7NamedVolumeMigrationSource> + 'operation>,
}

impl<'operation> V7NamedVolumeMigrationAdapter<'operation> {
    pub(super) fn new(
        options: V7NamedVolumeMigrationAdapterOptions<'operation>,
    ) -> Result<Self, String> {
        validate_accepted_source(&options)?;

        Ok(Self {
            source: options.source.clone(),
            provider: options.provider,
        })
    }
}

impl V7MigrationAdapterExecutor for V7NamedVolumeMigrationAdapter<'_> {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        self.provider.backup_source(&self.source)
    }

    fn prepare_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        if checkpoint.recovery_reference().is_none()
            || checkpoint.recovery_artifact_sha256().is_none()
            || checkpoint.recovery_artifact_size_bytes().is_none()
        {
            return Box::pin(async {
                Err(MigrationOperationError::new(
                    "named-volume target requires verified recovery evidence",
                ))
            });
        }
        self.provider
            .restore_and_verify_target(&self.source, checkpoint)
    }

    fn cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        let Some(target_reference) = checkpoint.target_reference() else {
            return Box::pin(async {
                Err(MigrationOperationError::new(
                    "named-volume checkpoint has no prepared target",
                ))
            });
        };
        self.provider.verify_target(&self.source, target_reference)
    }

    fn rollback<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.provider.verify_source(&self.source)
    }

    fn confirm<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.provider.retire_source(&self.source)
    }
}

fn validate_accepted_source(
    options: &V7NamedVolumeMigrationAdapterOptions<'_>,
) -> Result<(), String> {
    let inventory = serde_json::from_str::<serde_json::Value>(options.accepted.inventory_json())
        .map_err(|error| format!("accepted v7 volume evidence is invalid: {error}"))?;
    let services = inventory
        .get("services")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "accepted v7 inventory has no service evidence".to_owned())?;
    let matching = services
        .iter()
        .filter(|service| {
            service
                .get("service_id")
                .and_then(serde_json::Value::as_str)
                == Some(options.source.service_id())
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err("accepted v7 inventory has ambiguous named-volume service evidence".to_owned());
    }
    let service = matching[0];
    if service
        .get("observed_container_id")
        .and_then(serde_json::Value::as_str)
        != Some(options.source.container_id())
    {
        return Err("legacy volume container differs from accepted v7 evidence".to_owned());
    }
    let configured_volumes = accepted_named_volume_mounts(service, "configured_mounts")?;
    let observed_volumes = accepted_named_volume_mounts(service, "observed_mounts")?;
    if configured_volumes != options.source.mounts() || observed_volumes != options.source.mounts()
    {
        return Err("legacy named volumes differ from accepted v7 evidence".to_owned());
    }

    Ok(())
}

fn accepted_named_volume_mounts(
    service: &serde_json::Value,
    field: &str,
) -> Result<Vec<V7NamedVolumeMigrationMount>, String> {
    let mut mounts = service
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("accepted v7 service has no {field} evidence"))?
        .iter()
        .filter(|mount| {
            mount.get("source_kind").and_then(serde_json::Value::as_str) == Some("named_volume")
        })
        .map(|mount| {
            V7NamedVolumeMigrationMount::new(
                mount
                    .get("source")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
                mount
                    .get("target")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    mounts.sort_by(|left, right| left.volume_name().cmp(right.volume_name()));

    Ok(mounts)
}
