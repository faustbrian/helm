use super::{IpcV7GeneratedEnvironmentArtifact, IpcV7PublicFileArtifact};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Complete secret-free host artifact evidence for one legacy project.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcV7HostArtifactInventory {
    generated_environment: Option<IpcV7GeneratedEnvironmentArtifact>,
    hosts_path: PathBuf,
    hosts_domains: Vec<String>,
    caddy_state_path: PathBuf,
    caddy_routes: BTreeMap<String, String>,
    caddy_ca_certificates: Vec<IpcV7PublicFileArtifact>,
}

impl From<&crate::control_plane::migration::V7HostArtifactInventory>
    for IpcV7HostArtifactInventory
{
    fn from(artifacts: &crate::control_plane::migration::V7HostArtifactInventory) -> Self {
        Self {
            generated_environment: artifacts
                .generated_environment()
                .map(IpcV7GeneratedEnvironmentArtifact::from),
            hosts_path: artifacts.hosts_path().to_path_buf(),
            hosts_domains: artifacts.hosts_domains().to_vec(),
            caddy_state_path: artifacts.caddy_state_path().to_path_buf(),
            caddy_routes: artifacts.caddy_routes().clone(),
            caddy_ca_certificates: artifacts
                .caddy_ca_certificates()
                .iter()
                .map(IpcV7PublicFileArtifact::from)
                .collect(),
        }
    }
}

impl IpcV7HostArtifactInventory {
    pub(crate) const fn generated_environment(&self) -> Option<&IpcV7GeneratedEnvironmentArtifact> {
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

    pub(crate) fn caddy_ca_certificates(&self) -> &[IpcV7PublicFileArtifact] {
        &self.caddy_ca_certificates
    }
}
