//! cli handlers dump cmd module.
//!
//! Contains cli handlers dump cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::PathBuf;

use crate::{cli, config, database};

/// Handles the `dump` CLI command.
pub(crate) fn handle_dump(
    config: &config::Config,
    service: Option<&str>,
    file: Option<&PathBuf>,
    stdout: bool,
    gzip: bool,
) -> Result<()> {
    let svc = config::resolve_service(config, service)?;
    cli::support::ensure_sql_service(svc, "dump")?;

    if stdout {
        database::dump_stdout(svc, gzip)?;
    } else if let Some(path) = file {
        database::dump(svc, path, gzip)?;
    } else {
        anyhow::bail!("specify --file or --stdout");
    }

    Ok(())
}
