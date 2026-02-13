//! cli handlers artisan cmd runtime module.
//!
//! Contains cli handlers artisan cmd runtime logic used by Helm command workflows.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::{cli, config, docker, serve};

/// Ensures test services running exists and is in the required state.
pub(super) fn ensure_test_services_running(
    config: &config::Config,
    workspace_root: &Path,
    inferred_env: &HashMap<String, String>,
) -> Result<()> {
    let startup_services = cli::support::resolve_up_services(config, None, None, None)?;
    for svc in startup_services {
        if svc.kind == config::Kind::App {
            serve::run(
                svc,
                false,
                svc.trust_container_ca,
                true,
                workspace_root,
                inferred_env,
                true,
            )?;
        } else {
            docker::up(
                svc,
                docker::UpOptions {
                    pull: docker::PullPolicy::Missing,
                    recreate: false,
                },
            )?;
        }
    }
    Ok(())
}
