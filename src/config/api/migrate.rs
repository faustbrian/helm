//! config api migrate module.
//!
//! Contains config api migrate logic used by Helm command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::super::{expansion, validation};
use super::load_save::{load_raw_config_with, save_config_with};

pub fn migrate_config_with(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    let mut raw = load_raw_config_with(config_path, project_root)?;
    let version = raw.schema_version.unwrap_or(1);
    if version > 1 {
        anyhow::bail!("schema_version '{version}' is newer than this Helm build supports");
    }

    raw.schema_version = Some(1);
    let mut config = expansion::expand_raw_config(raw)?;
    validation::validate_and_resolve_container_names(&mut config)?;
    validation::validate_swarm_targets(&config)?;
    save_config_with(&config, config_path, project_root)
}
