//! cli handlers docker ops attach module.
//!
//! Contains attach handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(super) fn handle_attach(
    config: &config::Config,
    service: Option<&str>,
    no_stdin: bool,
    sig_proxy: bool,
    detach_keys: Option<&str>,
) -> Result<()> {
    let svc = config::resolve_service(config, service)?;
    docker::attach(svc, no_stdin, sig_proxy, detach_keys)
}
