//! docker manage module.
//!
//! Contains docker manage logic used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

mod container_ops;
mod image_ops;

/// Downs down as part of the docker manage workflow.
pub fn down(service: &ServiceConfig) -> Result<()> {
    container_ops::down(service)
}

/// Stops stop as part of the docker manage workflow.
pub fn stop(service: &ServiceConfig) -> Result<()> {
    container_ops::stop(service)
}

/// Rms rm as part of the docker manage workflow.
pub fn rm(service: &ServiceConfig, force: bool) -> Result<()> {
    container_ops::rm(service, force)
}

pub fn recreate(service: &ServiceConfig) -> Result<()> {
    container_ops::recreate(service)
}

/// Pulls pull as part of the docker manage workflow.
pub fn pull(service: &ServiceConfig) -> Result<()> {
    image_ops::pull(service)
}

/// Restarts restart as part of the docker manage workflow.
pub fn restart(service: &ServiceConfig) -> Result<()> {
    container_ops::restart(service)
}
