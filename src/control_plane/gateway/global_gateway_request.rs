use super::GlobalGatewayRequestOptions;
use crate::control_plane::engine::{
    ContainerCreateOptions, EngineError, GatewayContainerRequestOptions, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, gateway_container_request,
};
use sha2::{Digest, Sha256};

const GATEWAY_IMAGE: &str = concat!(
    "caddy@sha256:",
    "af5fdcd76f2db5e4e974ee92f96ee8c0fc3edb55bd4ba5032547cbf3f65e486d"
);
const GATEWAY_NETWORK: &str = "stackctl";
const GATEWAY_PROFILE: &str = "caddy-json-v1";
const GATEWAY_RESOURCE_ID: &str = "gateway";
const GATEWAY_SCHEMA_VERSION: u32 = 8;

/// Builds the immutable official-Caddy request for the one global gateway.
pub(crate) fn global_gateway_request(
    options: GlobalGatewayRequestOptions,
) -> Result<ContainerCreateOptions, EngineError> {
    if options.certificate_revision.is_empty() {
        return Err(EngineError::InvalidRequest {
            detail: "gateway certificate revision must not be empty".to_owned(),
        });
    }
    let desired_revision = gateway_revision(&options.certificate_revision, &options.container_user);
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id,
        kind: ResourceKind::Gateway,
        project_id: None,
        compatibility_fingerprint: GATEWAY_PROFILE.to_owned(),
        schema_version: GATEWAY_SCHEMA_VERSION,
        desired_revision,
        retention: RetentionClass::Disposable,
    })?
    .with_resource_id(GATEWAY_RESOURCE_ID)?;

    gateway_container_request(GatewayContainerRequestOptions::new(
        GATEWAY_IMAGE.to_owned(),
        GATEWAY_NETWORK.to_owned(),
        options.container_user,
        options.certificate_path,
        options.private_key_path,
        options.bootstrap_config_path,
        metadata,
    ))
}

fn gateway_revision(certificate_revision: &str, container_user: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"stackctl-global-gateway-v1\0");
    hasher.update(GATEWAY_IMAGE.as_bytes());
    hasher.update(b"\0");
    hasher.update(certificate_revision.as_bytes());
    hasher.update(b"\0");
    hasher.update(container_user.as_bytes());

    format!("sha256:{}", hex::encode(hasher.finalize()))
}
