use super::GatewayError;
use std::net::IpAddr;

/// Replaceable host resolver used only for the `.localhost` safety check.
pub(crate) trait LocalhostResolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, GatewayError>;
}
