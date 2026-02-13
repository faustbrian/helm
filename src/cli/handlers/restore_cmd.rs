//! cli handlers restore cmd module.
//!
//! Contains cli handlers restore cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::{cli, config, database};

/// Handles the `restore` CLI command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_restore(
    config: &config::Config,
    service: Option<&str>,
    file: Option<&PathBuf>,
    reset: bool,
    migrate: bool,
    schema_dump: bool,
    gzip: bool,
    project_root: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<()> {
    let svc = config::resolve_service(config, service)?;
    cli::support::ensure_sql_service(svc, "restore")?;

    match file {
        Some(path) => database::restore(svc, path, reset, gzip)?,
        None => database::restore_stdin(svc, reset, gzip)?,
    }

    if migrate || schema_dump {
        database::run_laravel_post_restore(migrate, schema_dump, project_root, config_path)?;
    }

    Ok(())
}
