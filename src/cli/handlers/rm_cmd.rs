//! cli handlers rm cmd module.
//!
//! Contains cli handlers rm cmd logic used by Helm command workflows.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

/// Handles the `rm` CLI command.
pub(crate) fn handle_rm(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    force: bool,
    parallel: usize,
    quiet: bool,
) -> Result<()> {
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        if !quiet {
            output::event(
                &svc.name,
                LogLevel::Info,
                "Removing service container",
                Persistence::Persistent,
            );
        }
        docker::rm(svc, force)
    })
}
