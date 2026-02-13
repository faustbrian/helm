//! docker ops stats module.
//!
//! Contains docker stats operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn stats(service: &ServiceConfig, no_stream: bool, format: Option<&str>) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["stats".to_owned()];
    if no_stream {
        args.push("--no-stream".to_owned());
    }
    if let Some(template) = format {
        args.push("--format".to_owned());
        args.push(template.to_owned());
    }
    args.push(container_name);
    run_docker_status(&args, "Failed to execute docker stats command")
}
