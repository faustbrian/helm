use anyhow::Result;

use crate::config::ServiceConfig;

mod common;
mod create;
mod dump;
mod reset;

pub(super) fn create_database(service: &ServiceConfig) -> Result<()> {
    create::create_database(service)
}

pub(super) fn reset_database(service: &ServiceConfig) -> Result<()> {
    reset::reset_database(service)
}

pub(super) fn run_dump_command(service: &ServiceConfig) -> Result<std::process::Output> {
    dump::run_dump_command(service)
}
