use anyhow::Result;

use crate::config::ServiceConfig;

mod container_ops;
mod image_ops;

pub fn down(service: &ServiceConfig) -> Result<()> {
    container_ops::down(service)
}

pub fn stop(service: &ServiceConfig) -> Result<()> {
    container_ops::stop(service)
}

pub fn rm(service: &ServiceConfig, force: bool) -> Result<()> {
    container_ops::rm(service, force)
}

pub fn recreate(service: &ServiceConfig) -> Result<()> {
    container_ops::recreate(service)
}

pub fn pull(service: &ServiceConfig) -> Result<()> {
    image_ops::pull(service)
}

pub fn restart(service: &ServiceConfig) -> Result<()> {
    container_ops::restart(service)
}
