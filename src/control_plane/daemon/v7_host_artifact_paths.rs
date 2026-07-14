use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Explicit host paths used by isolated v7 artifact compatibility discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7HostArtifactPaths {
    environment_path: PathBuf,
    hosts_path: PathBuf,
    caddy_state_path: PathBuf,
    caddy_ca_candidates: Vec<PathBuf>,
}

impl V7HostArtifactPaths {
    pub(crate) fn new(
        environment_path: PathBuf,
        hosts_path: PathBuf,
        caddy_state_path: PathBuf,
        caddy_ca_candidates: Vec<PathBuf>,
    ) -> Self {
        Self {
            environment_path,
            hosts_path,
            caddy_state_path,
            caddy_ca_candidates,
        }
    }

    pub(crate) fn for_current_user(canonical_project_path: &Path) -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set for legacy host-artifact inventory".to_owned())?;
        let relative_ca = Path::new("caddy/pki/authorities/local/root.crt");
        let mut caddy_ca_candidates = BTreeSet::new();
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
            caddy_ca_candidates.insert(data_home.join(relative_ca));
        }
        caddy_ca_candidates.insert(home.join(".local/share").join(relative_ca));
        caddy_ca_candidates.insert(home.join("Library/Application Support").join(relative_ca));

        Ok(Self::new(
            canonical_project_path.join(".env"),
            PathBuf::from("/etc/hosts"),
            home.join(".config/stackctl/caddy/sites.toml"),
            caddy_ca_candidates.into_iter().collect(),
        ))
    }

    pub(crate) fn environment_path(&self) -> &Path {
        &self.environment_path
    }

    pub(crate) fn hosts_path(&self) -> &Path {
        &self.hosts_path
    }

    pub(crate) fn caddy_state_path(&self) -> &Path {
        &self.caddy_state_path
    }

    pub(crate) fn caddy_ca_candidates(&self) -> &[PathBuf] {
        &self.caddy_ca_candidates
    }
}
