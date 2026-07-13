use super::{IdentityError, ProjectIdentity, RouteIdentity, ServiceIdentity};
use std::path::{Path, PathBuf};

/// One canonical project path's claim to a deterministic service route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RouteClaim {
    canonical_project_path: PathBuf,
    project: ProjectIdentity,
    service: ServiceIdentity,
    route: RouteIdentity,
}

impl RouteClaim {
    /// Creates a claim from a path canonicalized by the discovery boundary.
    pub(crate) fn new(
        canonical_project_path: PathBuf,
        project: ProjectIdentity,
        service: ServiceIdentity,
    ) -> Result<Self, IdentityError> {
        let route = RouteIdentity::new(&project, &service)?;

        Ok(Self {
            canonical_project_path,
            project,
            service,
            route,
        })
    }

    /// Returns the canonical project path supplied by discovery.
    pub(crate) fn canonical_project_path(&self) -> &Path {
        &self.canonical_project_path
    }

    /// Returns the exact project identity.
    pub(crate) fn project_name(&self) -> &str {
        self.project.as_str()
    }

    /// Returns the exact service identity.
    pub(crate) fn service_name(&self) -> &str {
        self.service.as_str()
    }

    /// Returns the deterministic route domain.
    pub(crate) fn domain(&self) -> &str {
        self.route.domain()
    }
}
