use anyhow::Result;

use crate::config::ServiceConfig;

mod down;
mod recreate;
mod restart;
mod rm;
mod stop;

pub(super) fn down(service: &ServiceConfig) -> Result<()> {
    down::down(service)
}

pub(super) fn stop(service: &ServiceConfig) -> Result<()> {
    stop::stop(service)
}

pub(super) fn rm(service: &ServiceConfig, force: bool) -> Result<()> {
    rm::rm(service, force)
}

pub(super) fn recreate(service: &ServiceConfig) -> Result<()> {
    recreate::recreate(service)
}

pub(super) fn restart(service: &ServiceConfig) -> Result<()> {
    restart::restart(service)
}
