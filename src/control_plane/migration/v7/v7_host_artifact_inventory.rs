use super::{V7GeneratedEnvironmentArtifact, V7PublicFileArtifact};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Secret-free legacy host state needed for reversible route and trust cutover.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct V7HostArtifactInventory {
    generated_environment: Option<V7GeneratedEnvironmentArtifact>,
    hosts_path: PathBuf,
    hosts_domains: Vec<String>,
    caddy_state_path: PathBuf,
    caddy_routes: BTreeMap<String, String>,
    caddy_ca_certificates: Vec<V7PublicFileArtifact>,
}

impl V7HostArtifactInventory {
    pub(super) fn new(
        generated_environment: Option<V7GeneratedEnvironmentArtifact>,
        hosts_path: PathBuf,
        hosts_domains: Vec<String>,
        caddy_state_path: PathBuf,
        caddy_routes: BTreeMap<String, String>,
        caddy_ca_certificates: Vec<V7PublicFileArtifact>,
    ) -> Self {
        Self {
            generated_environment,
            hosts_path,
            hosts_domains,
            caddy_state_path,
            caddy_routes,
            caddy_ca_certificates,
        }
    }

    pub(crate) const fn generated_environment(&self) -> Option<&V7GeneratedEnvironmentArtifact> {
        self.generated_environment.as_ref()
    }

    pub(crate) fn hosts_path(&self) -> &Path {
        &self.hosts_path
    }

    pub(crate) fn hosts_domains(&self) -> &[String] {
        &self.hosts_domains
    }

    pub(crate) fn caddy_state_path(&self) -> &Path {
        &self.caddy_state_path
    }

    pub(crate) fn caddy_routes(&self) -> &BTreeMap<String, String> {
        &self.caddy_routes
    }

    pub(crate) fn caddy_ca_certificates(&self) -> &[V7PublicFileArtifact] {
        &self.caddy_ca_certificates
    }
}
