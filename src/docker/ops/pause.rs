//! docker ops pause module.
//!
//! Contains docker pause operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_simple_container_command;

pub(super) fn pause(service: &ServiceConfig) -> Result<()> {
    run_simple_container_command(service, "pause", "Failed to execute docker pause command")
}
