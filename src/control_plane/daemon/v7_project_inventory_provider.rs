use super::ipc::IpcV7ProjectInventory;
use crate::control_plane::migration::V7GeneratedEnvironmentRollbackMaterial;
use std::path::Path;

/// Synchronous IPC boundary for one bounded explicit legacy inventory scan.
pub(crate) trait V7ProjectInventoryProvider {
    fn inventory(
        &mut self,
        canonical_project_path: &Path,
        maximum_config_bytes: usize,
    ) -> Result<IpcV7ProjectInventory, String>;

    fn capture_generated_environment_rollback(
        &mut self,
        inventory: &IpcV7ProjectInventory,
        _evidence_revision: &str,
        _maximum_environment_bytes: usize,
        _created_at_unix_seconds: i64,
    ) -> Result<Option<V7GeneratedEnvironmentRollbackMaterial>, String> {
        if inventory.host_artifacts().generated_environment().is_some() {
            return Err(
                "legacy inventory provider cannot capture protected environment rollback"
                    .to_owned(),
            );
        }

        Ok(None)
    }
}
