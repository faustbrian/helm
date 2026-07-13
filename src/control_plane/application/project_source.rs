use std::path::{Path, PathBuf};

/// One discovered canonical project and its unread, untrusted YAML source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectSource {
    canonical_path: PathBuf,
    config_path: PathBuf,
    yaml: String,
}

impl ProjectSource {
    /// Creates an effect-free discovery input for registry planning.
    pub(crate) fn new(canonical_path: PathBuf, config_path: PathBuf, yaml: String) -> Self {
        Self {
            canonical_path,
            config_path,
            yaml,
        }
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub(super) fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub(super) fn yaml(&self) -> &str {
        &self.yaml
    }
}
