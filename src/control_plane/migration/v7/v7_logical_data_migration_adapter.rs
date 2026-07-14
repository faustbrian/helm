use super::{
    V7LogicalDataMigrationAdapterOptions, V7LogicalDataMigrationSource, V7MigrationAdapterExecutor,
    V7MigrationAdapterTarget, V7RecoverableMigrationProvider,
};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture, MigrationOperationError};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;

/// Runs one accepted logical-data transition through its driver provider.
pub(super) struct V7LogicalDataMigrationAdapter<'operation> {
    source: &'operation V7LogicalDataMigrationSource,
    provider: &'operation mut dyn V7RecoverableMigrationProvider<V7LogicalDataMigrationSource>,
}

impl<'operation> V7LogicalDataMigrationAdapter<'operation> {
    pub(super) fn new(
        options: V7LogicalDataMigrationAdapterOptions<'operation>,
    ) -> Result<Self, String> {
        validate_accepted_source(&options)?;

        Ok(Self {
            source: options.source,
            provider: options.provider,
        })
    }
}

impl V7MigrationAdapterExecutor for V7LogicalDataMigrationAdapter<'_> {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        self.provider.backup_source(self.source)
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
                    "logical-data target requires verified recovery evidence",
                ))
            });
        }
        self.provider
            .restore_and_verify_target(self.source, checkpoint)
    }

    fn cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        let Some(target_reference) = checkpoint.target_reference() else {
            return Box::pin(async {
                Err(MigrationOperationError::new(
                    "logical-data checkpoint has no prepared target",
                ))
            });
        };
        self.provider.verify_target(self.source, target_reference)
    }

    fn rollback<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.provider.verify_source(self.source)
    }

    fn confirm<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.provider.retire_source(self.source)
    }
}

fn validate_accepted_source(
    options: &V7LogicalDataMigrationAdapterOptions<'_>,
) -> Result<(), String> {
    let inventory = serde_json::from_str::<serde_json::Value>(options.accepted.inventory_json())
        .map_err(|error| format!("accepted v7 logical-data evidence is invalid: {error}"))?;
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
        return Err("accepted v7 inventory has ambiguous logical-data service evidence".to_owned());
    }
    let service = matching[0];
    if service.get("driver").and_then(serde_json::Value::as_str) != Some(options.source.driver())
        || service
            .get("observed_container_id")
            .and_then(serde_json::Value::as_str)
            != Some(options.source.container_id())
    {
        return Err("legacy logical-data source differs from accepted v7 evidence".to_owned());
    }
    let logical_data = service
        .get("logical_data")
        .cloned()
        .ok_or_else(|| "accepted v7 service has no logical-data evidence".to_owned())?;
    let logical_data =
        serde_json::from_value::<std::collections::BTreeMap<String, String>>(logical_data)
            .map_err(|error| format!("accepted v7 logical-data identity is invalid: {error}"))?;
    if &logical_data != options.source.logical_data() {
        return Err("legacy logical-data identity differs from accepted v7 evidence".to_owned());
    }

    Ok(())
}
