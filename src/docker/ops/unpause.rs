//! docker ops unpause module.
//!
//! Contains docker unpause operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn unpause(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;
    let args = vec!["unpause".to_owned(), container_name];
    run_docker_status(&args, "Failed to execute docker unpause command")
}
