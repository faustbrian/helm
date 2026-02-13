//! cli handlers env cmd managed module.
//!
//! Contains cli handlers env cmd managed logic used by Helm command workflows.

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, env};

mod persist;
mod runtime_sync;

use persist::{collect_persist_targets, persist_runtime_host_ports};
use runtime_sync::sync_managed_values_from_runtime;

/// Handles the `managed env update` CLI command.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_managed_env_update(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    env_path: &Path,
    sync: bool,
    purge: bool,
    persist_runtime: bool,
    create_missing: bool,
    quiet: bool,
    config_path: &Option<PathBuf>,
    project_root: &Option<PathBuf>,
) -> Result<()> {
    let selected = cli::support::selected_services(config, service, kind, None)?;
    let mut selected_names = HashSet::new();
    for svc in &selected {
        selected_names.insert(svc.name.clone());
    }
    let persist_targets = collect_persist_targets(&selected);

    let mut managed_values = env::managed_app_env(config);
    let managed_keys: HashSet<String> = managed_values.keys().cloned().collect();

    if sync {
        sync_managed_values_from_runtime(
            config,
            &selected_names,
            &managed_keys,
            &mut managed_values,
        );
    }

    env::write_env_values_with_purge(
        env_path,
        &managed_values,
        create_missing,
        &managed_keys,
        purge,
    )?;

    if persist_runtime {
        persist_runtime_host_ports(config, &persist_targets, quiet, config_path, project_root)?;
    }

    if !quiet {
        output::event(
            "env",
            LogLevel::Success,
            &format!(
                "Updated {} with managed app env{}{}{}",
                env_path.display(),
                if sync {
                    " (synced from runtime where available)"
                } else {
                    ""
                },
                if purge {
                    " and purged stale managed keys"
                } else {
                    ""
                },
                if persist_runtime {
                    " and persisted runtime host/port to .helm.toml"
                } else {
                    ""
                }
            ),
            Persistence::Persistent,
        );
    }

    Ok(())
}
