use super::{GatewayError, GatewayRoute};
use sha2::{Digest, Sha256};

/// The complete immutable route set applied as one gateway transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GatewaySnapshot {
    revision: String,
    routes: Vec<GatewayRoute>,
}

impl GatewaySnapshot {
    pub(crate) fn new(mut routes: Vec<GatewayRoute>) -> Result<Self, GatewayError> {
        routes.sort();

        if let Some(domain) = duplicate_domain(&routes) {
            return Err(GatewayError::InvalidPlan {
                detail: format!("gateway domain '{domain}' has multiple upstreams"),
            });
        }

        let revision = route_revision(&routes);

        Ok(Self { revision, routes })
    }

    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }

    pub(crate) fn routes(&self) -> &[GatewayRoute] {
        &self.routes
    }
}

fn route_revision(routes: &[GatewayRoute]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"stackctl-gateway-routes-v1\0");
    for route in routes {
        digest.update(route.domain().as_bytes());
        digest.update([0]);
        digest.update(route.upstream().as_bytes());
        digest.update([0]);
    }

    format!("sha256:{}", hex::encode(digest.finalize()))
}

fn duplicate_domain(routes: &[GatewayRoute]) -> Option<&str> {
    routes.windows(2).find_map(|pair| {
        let first = pair.first()?;
        let second = pair.get(1)?;

        (first.domain() == second.domain()).then(|| first.domain())
    })
}
