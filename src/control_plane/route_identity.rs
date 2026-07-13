use super::{IdentityError, ProjectIdentity, ServiceIdentity};

const DOMAIN_SUFFIX: &str = ".stackctl.localhost";
const MAX_DNS_LABEL_BYTES: usize = 63;

/// The deterministic route owned by one project service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RouteIdentity {
    domain: String,
}

impl RouteIdentity {
    /// Builds `{project}-{service}.stackctl.localhost` without fallback names.
    pub(crate) fn new(
        project: &ProjectIdentity,
        service: &ServiceIdentity,
    ) -> Result<Self, IdentityError> {
        let label = format!("{}-{}", project.as_str(), service.as_str());

        if label.len() > MAX_DNS_LABEL_BYTES {
            return Err(IdentityError::RouteLabelTooLong { label });
        }

        Ok(Self {
            domain: format!("{label}{DOMAIN_SUFFIX}"),
        })
    }

    /// Returns the exact deterministic route domain.
    pub(crate) fn domain(&self) -> &str {
        &self.domain
    }
}
