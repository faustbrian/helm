//! docker ops inspect module.
//!
//! Contains docker inspect operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn inspect_container(
    service: &ServiceConfig,
    format: Option<&str>,
    size: bool,
    object_type: Option<&str>,
) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["inspect".to_owned()];
    if let Some(template) = format {
        args.push("--format".to_owned());
        args.push(template.to_owned());
    }
    if size {
        args.push("--size".to_owned());
    }
    if let Some(kind) = object_type {
        args.push("--type".to_owned());
        args.push(kind.to_owned());
    }
    args.push(container_name);
    run_docker_status(&args, "Failed to execute docker inspect command")
}
