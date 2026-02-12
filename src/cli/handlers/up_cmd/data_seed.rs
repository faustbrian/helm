use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::{cli, config, database};

pub(super) fn apply_data_seeds(
    config_data: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    workspace_root: &Path,
    quiet: bool,
) -> Result<()> {
    let selected: Vec<&config::ServiceConfig> = if let Some(profile_name) = profile {
        cli::support::resolve_profile_targets(config_data, profile_name)?
    } else {
        cli::support::selected_services(config_data, service, kind, None)?
    };

    for svc in selected {
        if svc.kind != config::Kind::Database {
            continue;
        }
        let Some(seed_file) = svc.seed_file.as_deref() else {
            continue;
        };
        let seed_path = resolve_seed_path(workspace_root, seed_file);
        let gzip = seed_path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));

        database::restore(svc, &seed_path, false, gzip)?;
        if !quiet {
            println!(
                "Applied data seed for '{}' from {}",
                svc.name,
                seed_path.display()
            );
        }
    }

    Ok(())
}

fn resolve_seed_path(workspace_root: &Path, seed_file: &str) -> PathBuf {
    let path = PathBuf::from(seed_file);
    if path.is_absolute() {
        return path;
    }
    workspace_root.join(path)
}
