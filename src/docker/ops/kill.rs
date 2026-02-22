//! docker ops kill module.
//!
//! Contains docker kill operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_container_command_with_optional_flag;

pub(super) fn kill(service: &ServiceConfig, signal: Option<&str>) -> Result<()> {
    run_container_command_with_optional_flag(
        service,
        "kill",
        "--signal",
        signal,
        "Failed to execute docker kill command",
    )
}
