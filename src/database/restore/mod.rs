use anyhow::Result;

use std::path::Path;

use crate::config::ServiceConfig;

use super::setup::ensure_sql_service;
use super::sql_admin::reset_database;

mod from_file;
mod from_stdin;
mod process;

pub(crate) fn restore(
    service: &ServiceConfig,
    file_path: &Path,
    reset: bool,
    gzip: bool,
) -> Result<()> {
    ensure_sql_service(service)?;

    if reset {
        reset_database(service)?;
    }

    from_file::restore_from_file(service, file_path, gzip)
}

pub(crate) fn restore_stdin(service: &ServiceConfig, reset: bool, gzip: bool) -> Result<()> {
    ensure_sql_service(service)?;

    if reset {
        reset_database(service)?;
    }

    from_stdin::restore_from_stdin(service, gzip)
}
