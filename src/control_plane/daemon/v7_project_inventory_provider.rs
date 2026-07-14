use super::ipc::IpcV7ProjectInventory;
use std::path::Path;

/// Synchronous IPC boundary for one bounded explicit legacy inventory scan.
pub(crate) trait V7ProjectInventoryProvider {
    fn inventory(
        &mut self,
        canonical_project_path: &Path,
        maximum_config_bytes: usize,
    ) -> Result<IpcV7ProjectInventory, String>;
}
