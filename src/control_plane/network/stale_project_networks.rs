use crate::control_plane::engine::{
    ObservedNetwork, ObservedResourceOwnership, OwnedNetwork, ResourceKind, RetentionClass,
    reconstruct_owned_network,
};
use std::collections::BTreeSet;

/// Selects exact owned project networks whose project left desired state.
pub(crate) fn stale_project_networks(
    observed: &[ObservedNetwork],
    installation_id: &str,
    schema_version: u32,
    active_project_ids: &BTreeSet<String>,
) -> Result<Vec<OwnedNetwork>, String> {
    let mut stale = Vec::new();

    for observed_network in observed {
        let network =
            match reconstruct_owned_network(observed_network, installation_id, schema_version) {
                Ok(network) => network,
                Err(ObservedResourceOwnership::ForeignInstallation { .. })
                | Err(ObservedResourceOwnership::Unmanaged) => continue,
                Err(ownership) => {
                    return Err(format!(
                        "managed network ownership cannot be proven: {ownership:?}"
                    ));
                }
            };
        let Some(project_id) = network.metadata().project_id() else {
            continue;
        };
        if network.metadata().kind() != ResourceKind::Network
            || network.metadata().resource_id() != Some("private")
            || network.metadata().compatibility_fingerprint() != "network-v1"
            || network.metadata().desired_revision() != "network-v1"
            || network.metadata().retention() != RetentionClass::Persistent
        {
            return Err(format!(
                "project '{}' owns a noncanonical managed network",
                project_id
            ));
        }
        if !active_project_ids.contains(project_id) {
            stale.push(network);
        }
    }

    Ok(stale)
}
