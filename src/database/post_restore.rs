//! database post restore module.
//!
//! Contains database post restore logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};

/// Runs optional post-restore artisan hooks for the active app service.
pub(crate) fn run_laravel_post_restore(
    run_migrate: bool,
    run_schema_dump: bool,
    project_root_override: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<()> {
    let project_root = crate::config::project_root_with(config_path, project_root_override)?;

    if run_migrate {
        run_artisan_command(&project_root, "migrate")?;
    }

    if run_schema_dump {
        run_artisan_command(&project_root, "schema:dump")?;
    }

    Ok(())
}

/// Runs a single artisan command via `helm artisan`.
fn run_artisan_command(project_root: &Path, artisan_command: &str) -> Result<()> {
    output::event(
        "database",
        LogLevel::Info,
        &format!("Running `helm artisan {artisan_command}`"),
        Persistence::Persistent,
    );

    if crate::docker::is_dry_run() {
        output::event(
            "database",
            LogLevel::Info,
            &format!("[dry-run] helm artisan {artisan_command}"),
            Persistence::Transient,
        );
        return Ok(());
    }

    let config = crate::config::load_config_with(None, Some(project_root))
        .context("failed to load helm config for artisan post-restore hook")?;
    let app_service = crate::config::resolve_app_service(&config, None)
        .context("failed to resolve app service for artisan post-restore hook")?;
    let args = vec![artisan_command.to_owned()];
    crate::serve::exec_artisan(app_service, &args, false)
        .context("Failed to execute helm artisan command")?;

    output::event(
        "database",
        LogLevel::Success,
        &format!("`helm artisan {artisan_command}` completed"),
        Persistence::Persistent,
    );
    Ok(())
}
