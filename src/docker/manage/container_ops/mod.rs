//! docker manage container ops module.
//!
//! Contains docker manage container ops logic used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

mod down;
mod recreate;
mod restart;
mod rm;
mod stop;

/// Downs down as part of the docker manage container ops workflow.
pub(super) fn down(service: &ServiceConfig) -> Result<()> {
    down::down(service)
}

/// Stops stop as part of the docker manage container ops workflow.
pub(super) fn stop(service: &ServiceConfig) -> Result<()> {
    stop::stop(service)
}

/// Rms rm as part of the docker manage container ops workflow.
pub(super) fn rm(service: &ServiceConfig, force: bool) -> Result<()> {
    rm::rm(service, force)
}

pub(super) fn recreate(service: &ServiceConfig) -> Result<()> {
    recreate::recreate(service)
}

/// Restarts restart as part of the docker manage container ops workflow.
pub(super) fn restart(service: &ServiceConfig) -> Result<()> {
    restart::restart(service)
}
