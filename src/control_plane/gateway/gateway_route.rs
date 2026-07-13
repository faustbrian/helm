use super::GatewayError;

/// One validated public domain to internal plain-HTTP upstream mapping.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct GatewayRoute {
    domain: String,
    upstream: String,
}

impl GatewayRoute {
    pub(crate) fn new(
        domain: impl Into<String>,
        upstream: impl Into<String>,
    ) -> Result<Self, GatewayError> {
        let domain = domain.into();
        let upstream = upstream.into();

        if domain.is_empty() || !domain.ends_with(".stackctl.localhost") {
            return Err(GatewayError::InvalidPlan {
                detail: format!("gateway domain '{domain}' must end with '.stackctl.localhost'"),
            });
        }

        if !upstream.starts_with("http://") {
            return Err(GatewayError::InvalidPlan {
                detail: format!("gateway upstream '{upstream}' must use internal plain HTTP"),
            });
        }

        Ok(Self { domain, upstream })
    }

    pub(crate) fn domain(&self) -> &str {
        &self.domain
    }

    pub(crate) fn upstream(&self) -> &str {
        &self.upstream
    }
}
