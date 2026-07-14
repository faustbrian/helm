use std::path::{Path, PathBuf};

/// Explicit bounded filesystem inputs for one legacy host-artifact scan.
pub(crate) struct V7HostArtifactDiscoveryOptions<'inventory> {
    pub(crate) environment_path: &'inventory Path,
    pub(crate) hosts_path: &'inventory Path,
    pub(crate) caddy_state_path: &'inventory Path,
    pub(crate) caddy_ca_candidates: &'inventory [PathBuf],
    pub(crate) route_domains: &'inventory [String],
    pub(crate) maximum_artifact_bytes: usize,
}
