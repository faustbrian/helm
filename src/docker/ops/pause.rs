//! docker ops pause module.
//!
//! Contains docker pause operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn pause(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;
    let args = vec!["pause".to_owned(), container_name];
    run_docker_status(&args, "Failed to execute docker pause command")
}
