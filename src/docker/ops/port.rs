//! docker ops port module.
//!
//! Contains docker port operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn port(service: &ServiceConfig, private_port: Option<&str>) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["port".to_owned(), container_name];
    if let Some(port_name) = private_port {
        args.push(port_name.to_owned());
    }
    run_docker_status(&args, "Failed to execute docker port command")
}
