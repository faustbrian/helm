use crate::config::Config;
use crate::control_plane::engine::ObservedContainer;
use std::path::Path;

/// Read-only legacy inputs used to prove one migration source.
pub(crate) struct V7ProjectInventoryOptions<'inventory> {
    pub(crate) project_id: &'inventory str,
    pub(crate) canonical_project_path: &'inventory Path,
    pub(crate) source_revision: &'inventory str,
    pub(crate) config: &'inventory Config,
    pub(crate) observed_containers: &'inventory [ObservedContainer],
}
