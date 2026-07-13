use super::{GatewayError, GatewayRoute};

/// The complete immutable route set applied as one gateway transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GatewaySnapshot {
    revision: String,
    routes: Vec<GatewayRoute>,
}

impl GatewaySnapshot {
    pub(crate) fn new(
        revision: impl Into<String>,
        mut routes: Vec<GatewayRoute>,
    ) -> Result<Self, GatewayError> {
        let revision = revision.into();

        if revision.is_empty() {
            return Err(GatewayError::InvalidPlan {
                detail: "gateway snapshot revision must not be empty".to_owned(),
            });
        }

        routes.sort();

        if let Some(domain) = duplicate_domain(&routes) {
            return Err(GatewayError::InvalidPlan {
                detail: format!("gateway domain '{domain}' has multiple upstreams"),
            });
        }

        Ok(Self { revision, routes })
    }

    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }

    pub(crate) fn routes(&self) -> &[GatewayRoute] {
        &self.routes
    }
}

fn duplicate_domain(routes: &[GatewayRoute]) -> Option<&str> {
    routes.windows(2).find_map(|pair| {
        let first = pair.first()?;
        let second = pair.get(1)?;

        (first.domain() == second.domain()).then(|| first.domain())
    })
}
