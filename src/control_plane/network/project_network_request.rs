use super::project_network_name;
use crate::control_plane::engine::{
    EngineError, ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions,
    ResourceKind, RetentionClass,
};

const NETWORK_COMPATIBILITY: &str = "network-v1";
const NETWORK_RESOURCE_ID: &str = "private";
const NETWORK_SCHEMA_VERSION: u32 = 8;

/// Builds the deterministic private Engine network request for one project.
pub(crate) fn project_network_request(
    installation_id: &str,
    global_network_name: &str,
    project_id: &str,
) -> Result<NetworkCreateOptions, EngineError> {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.to_owned(),
        kind: ResourceKind::Network,
        project_id: Some(project_id.to_owned()),
        compatibility_fingerprint: NETWORK_COMPATIBILITY.to_owned(),
        schema_version: NETWORK_SCHEMA_VERSION,
        desired_revision: NETWORK_COMPATIBILITY.to_owned(),
        retention: RetentionClass::Persistent,
    })?
    .with_resource_id(NETWORK_RESOURCE_ID)?;

    NetworkCreateOptions::new(
        project_network_name(global_network_name, project_id),
        metadata,
    )
}
