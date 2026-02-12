use anyhow::Result;
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, env};

pub(super) fn handle_service_env_update(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    env_path: &Path,
    create_missing: bool,
    quiet: bool,
) -> Result<()> {
    if let Some(name) = service {
        let svc = config::find_service(config, name)?;
        if cli::support::matches_filter(svc, kind, None) {
            env::update_env(svc, env_path, create_missing)?;
            if !quiet {
                output::event(
                    &svc.name,
                    LogLevel::Success,
                    &format!("Updated {} with service config", env_path.display()),
                    Persistence::Persistent,
                );
            }
        }
        return Ok(());
    }

    for svc in cli::support::filter_services(&config.service, kind, None) {
        env::update_env(svc, env_path, create_missing)?;
        if !quiet {
            output::event(
                &svc.name,
                LogLevel::Success,
                &format!("Updated {} with service config", env_path.display()),
                Persistence::Persistent,
            );
        }
    }

    Ok(())
}
