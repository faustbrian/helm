//! database sql admin module.
//!
//! Contains database sql admin logic used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

mod common;
mod create;
mod dump;
mod reset;

/// Creates database for downstream execution.
pub(super) fn create_database(service: &ServiceConfig) -> Result<()> {
    create::create_database(service)
}

pub(super) fn reset_database(service: &ServiceConfig) -> Result<()> {
    reset::reset_database(service)
}

/// Executes dump command for the selected database service.
pub(super) fn run_dump_command(service: &ServiceConfig) -> Result<std::process::Output> {
    dump::run_dump_command(service)
}
