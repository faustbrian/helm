//! cli handlers stop cmd module.
//!
//! Contains cli handlers stop cmd logic used by Helm command workflows.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

/// Handles the `stop` CLI command.
pub(crate) fn handle_stop(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
    quiet: bool,
) -> Result<()> {
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        if !quiet {
            output::event(
                &svc.name,
                LogLevel::Info,
                "Stopping service",
                Persistence::Persistent,
            );
        }
        docker::stop(svc)
    })
}
