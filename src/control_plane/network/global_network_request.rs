use crate::control_plane::engine::{
    EngineError, ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions,
    ResourceKind, RetentionClass,
};

const NETWORK_NAME: &str = "stackctl";
const NETWORK_RESOURCE_ID: &str = "private";
const NETWORK_COMPATIBILITY: &str = "network-v1";
const NETWORK_SCHEMA_VERSION: u32 = 8;

/// Builds the one deterministic private network request for an installation.
pub(crate) fn global_network_request(
    installation_id: &str,
) -> Result<NetworkCreateOptions, EngineError> {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.to_owned(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: NETWORK_COMPATIBILITY.to_owned(),
        schema_version: NETWORK_SCHEMA_VERSION,
        desired_revision: NETWORK_COMPATIBILITY.to_owned(),
        retention: RetentionClass::Persistent,
    })?
    .with_resource_id(NETWORK_RESOURCE_ID)?;

    NetworkCreateOptions::new(NETWORK_NAME, metadata)
}
