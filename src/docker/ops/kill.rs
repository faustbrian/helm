//! docker ops kill module.
//!
//! Contains docker kill operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn kill(service: &ServiceConfig, signal: Option<&str>) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["kill".to_owned()];
    if let Some(sig) = signal {
        args.push("--signal".to_owned());
        args.push(sig.to_owned());
    }
    args.push(container_name);
    run_docker_status(&args, "Failed to execute docker kill command")
}
