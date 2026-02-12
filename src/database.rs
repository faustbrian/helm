//! SQL database setup and restore operations.

#![allow(clippy::print_stdout)] // Operations need to print status

use anyhow::Result;
use std::path::Path;

use crate::config::ServiceConfig;

mod dump;
mod post_restore;
mod restore;
mod setup;
mod sql_admin;

pub fn setup(service: &ServiceConfig, timeout: u64) -> Result<()> {
    setup::setup(service, timeout)
}

pub fn restore(service: &ServiceConfig, file_path: &Path, reset: bool, gzip: bool) -> Result<()> {
    restore::restore(service, file_path, reset, gzip)
}

pub fn dump(service: &ServiceConfig, file_path: &Path, gzip: bool) -> Result<()> {
    dump::dump(service, file_path, gzip)
}

pub fn dump_stdout(service: &ServiceConfig, gzip: bool) -> Result<()> {
    dump::dump_stdout(service, gzip)
}

pub fn restore_stdin(service: &ServiceConfig, reset: bool, gzip: bool) -> Result<()> {
    restore::restore_stdin(service, reset, gzip)
}

pub fn run_laravel_post_restore(
    run_migrate: bool,
    run_schema_dump: bool,
    project_root_override: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<()> {
    post_restore::run_laravel_post_restore(
        run_migrate,
        run_schema_dump,
        project_root_override,
        config_path,
    )
}
