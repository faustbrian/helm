use super::{GatewayError, LocalhostResolver};
use std::collections::BTreeSet;
use std::net::{IpAddr, ToSocketAddrs};

/// Operating-system resolver for the Stackctl `.localhost` preflight.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemLocalhostResolver;

impl LocalhostResolver for SystemLocalhostResolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, GatewayError> {
        let addresses = (host, 0)
            .to_socket_addrs()
            .map_err(|error| GatewayError::Provider {
                detail: error.to_string(),
            })?;

        Ok(addresses
            .map(|address| address.ip())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
}
