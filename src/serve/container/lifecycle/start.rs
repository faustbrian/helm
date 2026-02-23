//! Container start helpers for serve lifecycle.

use anyhow::Result;

use super::docker_cmd;
use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

pub(super) fn start_new_container(target: &ServiceConfig, run_args: &[String]) -> Result<()> {
    let args = crate::docker::docker_arg_refs(run_args);
    docker_cmd::checked_output(
        &args,
        "failed to execute docker run for serve target",
        "failed to run serve container",
    )?;

    output::event(
        &target.name,
        LogLevel::Success,
        "Started container for serve target",
        Persistence::Persistent,
    );
    Ok(())
}
