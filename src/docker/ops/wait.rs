//! docker ops wait module.
//!
//! Contains docker wait operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn wait(service: &ServiceConfig, condition: Option<&str>) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["wait".to_owned()];
    if let Some(condition_name) = condition {
        args.push("--condition".to_owned());
        args.push(condition_name.to_owned());
    }
    args.push(container_name);
    run_docker_status(&args, "Failed to execute docker wait command")
}
