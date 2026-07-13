use super::{
    BindMount, ContainerCreateOptions, ContainerRestartPolicy, EngineError,
    ManagedResourceMetadata, PortBinding,
};
use std::path::Path;

const GATEWAY_CONTAINER_NAME: &str = "stackctl-gateway";
const GATEWAY_TLS_DIRECTORY: &str = "/etc/stackctl/tls";

/// Produces the complete Engine mutation request for the singleton gateway.
pub(crate) fn gateway_container_request(
    image: &str,
    network: &str,
    tls_directory: &Path,
    metadata: ManagedResourceMetadata,
) -> Result<ContainerCreateOptions, EngineError> {
    let tls_directory = tls_directory
        .to_str()
        .ok_or_else(|| EngineError::InvalidRequest {
            detail: format!(
                "gateway TLS path '{}' is not valid UTF-8",
                tls_directory.display()
            ),
        })?;

    let options = ContainerCreateOptions::new(GATEWAY_CONTAINER_NAME, image, metadata)?
        .with_network(network)?
        .with_port_binding(PortBinding::loopback(80, 80)?)
        .with_port_binding(PortBinding::loopback(443, 443)?)
        .with_bind_mount(BindMount::read_only(tls_directory, GATEWAY_TLS_DIRECTORY)?)
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped);

    Ok(options)
}
