//! cli handlers restart cmd module.
//!
//! Contains cli handlers restart cmd logic used by Helm command workflows.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

/// Handles the `restart` CLI command.
pub(crate) fn handle_restart(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    healthy: bool,
    timeout: u64,
    parallel: usize,
) -> Result<()> {
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        output::event(
            &svc.name,
            LogLevel::Info,
            "Restarting service",
            Persistence::Persistent,
        );
        docker::restart(svc)?;
        if healthy {
            docker::wait_until_healthy(svc, timeout, 2, None)?;
        }
        Ok(())
    })
}
