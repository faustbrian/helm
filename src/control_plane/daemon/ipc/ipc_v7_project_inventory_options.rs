use super::{IpcV7Route, IpcV7ServiceInventory};
use std::path::PathBuf;

/// Complete secret-free wire fields for one legacy project inventory.
pub(crate) struct IpcV7ProjectInventoryOptions {
    pub(crate) project_id: String,
    pub(crate) canonical_project_path: PathBuf,
    pub(crate) source_revision: String,
    pub(crate) schema_version: u32,
    pub(crate) services: Vec<IpcV7ServiceInventory>,
    pub(crate) routes: Vec<IpcV7Route>,
    pub(crate) blockers: Vec<String>,
    pub(crate) requires_legacy_ca_capture: bool,
}
