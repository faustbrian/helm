use crate::config::Config;
use std::path::Path;

/// Exact legacy project inputs before Engine-owned source discovery.
pub(crate) struct V7ProjectInventoryRequest<'inventory> {
    pub(crate) project_id: &'inventory str,
    pub(crate) canonical_project_path: &'inventory Path,
    pub(crate) source_revision: &'inventory str,
    pub(crate) config: &'inventory Config,
}
