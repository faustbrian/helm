use crate::control_plane::engine::{ObservedContainer, ObservedNetwork, OwnedContainer};
use std::collections::BTreeSet;

/// Complete observed and desired state for one stale-network cleanup pass.
pub(crate) struct StaleProjectNetworkCleanupOptions<'operation> {
    pub(crate) observed_networks: &'operation [ObservedNetwork],
    pub(crate) observed_containers: &'operation [ObservedContainer],
    pub(crate) gateway: &'operation OwnedContainer,
    pub(crate) active_project_ids: &'operation BTreeSet<String>,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
