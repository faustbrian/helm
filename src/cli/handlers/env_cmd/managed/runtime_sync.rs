//! cli handlers env cmd managed runtime sync module.
//!
//! Contains cli handlers env cmd managed runtime sync logic used by Helm command workflows.

use std::collections::{HashMap, HashSet};

use crate::{cli, config, docker};

pub(super) fn sync_managed_values_from_runtime(
    config: &config::Config,
    selected_names: &HashSet<String>,
    managed_keys: &HashSet<String>,
    managed_values: &mut HashMap<String, String>,
) {
    let app_targets: Vec<&config::ServiceConfig> = if selected_names.is_empty() {
        cli::support::app_services(config)
    } else {
        cli::support::app_services(config)
            .into_iter()
            .filter(|svc| selected_names.contains(&svc.name))
            .collect()
    };

    for target in app_targets {
        let Ok(container_name) = target.container_name() else {
            continue;
        };

        if docker::inspect_status(&container_name).as_deref() != Some("running") {
            continue;
        }

        let Some(runtime_vars) = docker::inspect_env(&container_name) else {
            continue;
        };

        for key in managed_keys {
            if let Some(value) = runtime_vars.get(key) {
                managed_values.insert(key.clone(), value.clone());
            }
        }
    }
}
