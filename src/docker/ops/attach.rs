//! docker ops attach module.
//!
//! Contains docker attach operation used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn attach(
    service: &ServiceConfig,
    no_stdin: bool,
    sig_proxy: bool,
    detach_keys: Option<&str>,
) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["attach".to_owned()];
    if no_stdin {
        args.push("--no-stdin".to_owned());
    }
    if sig_proxy {
        args.push("--sig-proxy=true".to_owned());
    }
    if let Some(keys) = detach_keys {
        args.push("--detach-keys".to_owned());
        args.push(keys.to_owned());
    }
    args.push(container_name);
    run_docker_status(&args, "Failed to execute docker attach command")
}
