#[cfg(test)]
mod tests;

mod caddy_gateway_document;
mod caddy_gateway_provider;
#[cfg(unix)]
mod caddy_unix_admin_client;
mod gateway_configuration;
mod gateway_document_loader;
mod gateway_error;
mod gateway_port_availability;
mod gateway_port_probe;
mod gateway_route;
mod gateway_snapshot;
mod localhost_resolver;
mod render_caddy_document;
mod store_caddy_bootstrap;
mod stored_gateway_bootstrap_paths;
mod system_gateway_port_probe;
mod system_localhost_resolver;
mod verify_gateway_ports_available;
mod verify_stackctl_localhost_resolution;

pub(crate) use caddy_gateway_document::CaddyGatewayDocument;
pub(crate) use caddy_gateway_provider::CaddyGatewayProvider;
#[cfg(unix)]
pub(crate) use caddy_unix_admin_client::CaddyUnixAdminClient;
pub(crate) use gateway_configuration::{GatewayConfiguration, GatewayFuture};
pub(crate) use gateway_document_loader::GatewayDocumentLoader;
pub(crate) use gateway_error::GatewayError;
pub(crate) use gateway_port_availability::GatewayPortAvailability;
pub(crate) use gateway_port_probe::GatewayPortProbe;
pub(crate) use gateway_route::GatewayRoute;
pub(crate) use gateway_snapshot::GatewaySnapshot;
pub(crate) use localhost_resolver::LocalhostResolver;
pub(crate) use render_caddy_document::render_caddy_document;
pub(crate) use store_caddy_bootstrap::store_caddy_bootstrap;
pub(crate) use stored_gateway_bootstrap_paths::StoredGatewayBootstrapPaths;
pub(crate) use system_gateway_port_probe::SystemGatewayPortProbe;
pub(crate) use system_localhost_resolver::SystemLocalhostResolver;
pub(crate) use verify_gateway_ports_available::verify_gateway_ports_available;
pub(crate) use verify_stackctl_localhost_resolution::verify_stackctl_localhost_resolution;
