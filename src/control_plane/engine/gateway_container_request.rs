use super::{
    BindMount, ContainerCreateOptions, ContainerHealthCheck, ContainerRestartPolicy, EngineError,
    GatewayContainerRequestOptions, PortBinding,
};
use std::time::Duration;

const GATEWAY_CONTAINER_NAME: &str = "stackctl-gateway";
const GATEWAY_CERTIFICATE_PATH: &str = "/etc/stackctl/tls/wildcard.crt";
const GATEWAY_PRIVATE_KEY_PATH: &str = "/etc/stackctl/tls/wildcard.key";
const GATEWAY_CONFIG_PATH: &str = "/etc/stackctl/config.json";
const GATEWAY_RUNTIME_DIRECTORY: &str = "/run/stackctl";

/// Produces the complete Engine mutation request for the singleton gateway.
pub(crate) fn gateway_container_request(
    options: GatewayContainerRequestOptions,
) -> Result<ContainerCreateOptions, EngineError> {
    let certificate_path = utf8_path("certificate", &options.certificate_path)?;
    let private_key_path = utf8_path("private key", &options.private_key_path)?;
    let bootstrap_config_path = utf8_path("bootstrap config", &options.bootstrap_config_path)?;
    let admin_runtime_directory =
        utf8_path("admin runtime directory", &options.admin_runtime_directory)?;

    let request =
        ContainerCreateOptions::new(GATEWAY_CONTAINER_NAME, options.image, options.metadata)?
            .with_network(options.network)?
            .with_port_binding(PortBinding::loopback(80, 80)?)
            .with_port_binding(PortBinding::loopback(443, 443)?)
            .with_bind_mount(BindMount::read_only(
                certificate_path,
                GATEWAY_CERTIFICATE_PATH,
            )?)
            .with_bind_mount(BindMount::read_only(
                private_key_path,
                GATEWAY_PRIVATE_KEY_PATH,
            )?)
            .with_bind_mount(BindMount::read_only(
                bootstrap_config_path,
                GATEWAY_CONFIG_PATH,
            )?)
            .with_bind_mount(BindMount::read_write(
                admin_runtime_directory,
                GATEWAY_RUNTIME_DIRECTORY,
            )?)
            .with_command(vec![
                "caddy".to_owned(),
                "run".to_owned(),
                "--config".to_owned(),
                GATEWAY_CONFIG_PATH.to_owned(),
            ])?
            .with_health_check(ContainerHealthCheck::new(
                vec![
                    "caddy".to_owned(),
                    "validate".to_owned(),
                    "--config".to_owned(),
                    GATEWAY_CONFIG_PATH.to_owned(),
                ],
                Duration::from_secs(30),
                Duration::from_secs(5),
                Duration::from_secs(10),
                3,
            )?)
            .with_restart_policy(ContainerRestartPolicy::UnlessStopped);

    Ok(request)
}

fn utf8_path<'path>(kind: &str, path: &'path std::path::Path) -> Result<&'path str, EngineError> {
    path.to_str().ok_or_else(|| EngineError::InvalidRequest {
        detail: format!(
            "gateway {kind} path '{}' is not valid UTF-8",
            path.display()
        ),
    })
}
