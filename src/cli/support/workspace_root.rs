//! shared workspace-root resolution helper.

use anyhow::Result;
use std::path::{Path, PathBuf};

pub(crate) fn workspace_root(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    if config_path.is_none() && project_root.is_none() {
        return crate::config::project_root();
    }

    crate::config::project_root_with(crate::config::ProjectRootPathOptions::new(
        config_path,
        project_root,
    ))
}
