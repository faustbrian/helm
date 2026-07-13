use super::DesiredService;
use crate::control_plane::{ProjectIdentity, RouteClaim};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A v8 project after pure identity and graph validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesiredProject {
    identity: ProjectIdentity,
    project_directory: PathBuf,
    services: BTreeMap<String, DesiredService>,
    startup_order: Vec<String>,
    route_claims: Vec<RouteClaim>,
}

impl DesiredProject {
    pub(super) fn new(
        identity: ProjectIdentity,
        project_directory: PathBuf,
        services: BTreeMap<String, DesiredService>,
        startup_order: Vec<String>,
        route_claims: Vec<RouteClaim>,
    ) -> Self {
        Self {
            identity,
            project_directory,
            services,
            startup_order,
            route_claims,
        }
    }

    /// Returns the exact validated project identity.
    pub(crate) fn project_name(&self) -> &str {
        self.identity.as_str()
    }

    pub(crate) const fn identity(&self) -> &ProjectIdentity {
        &self.identity
    }

    /// Returns the canonical project directory supplied by discovery.
    pub(crate) fn project_directory(&self) -> &Path {
        &self.project_directory
    }

    /// Returns exact service names in deterministic order.
    pub(crate) fn service_names(&self) -> Vec<&str> {
        self.services.keys().map(String::as_str).collect()
    }

    /// Returns one complete desired service by exact identity.
    pub(crate) fn service(&self, name: &str) -> Option<&DesiredService> {
        self.services.get(name)
    }

    /// Returns the dependency-safe deterministic startup order.
    pub(crate) fn startup_order(&self) -> &[String] {
        &self.startup_order
    }

    /// Returns deterministic domains in service identity order.
    pub(crate) fn route_domains(&self) -> Vec<&str> {
        self.route_claims.iter().map(RouteClaim::domain).collect()
    }

    /// Returns claims ready for complete-registry collision validation.
    pub(crate) fn route_claims(&self) -> &[RouteClaim] {
        &self.route_claims
    }
}
