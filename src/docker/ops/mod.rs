//! docker ops module.
//!
//! Contains ad-hoc Docker command operations used by Helm command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

mod attach;
mod common;
mod cp;
mod events;
mod inspect;
mod kill;
mod pause;
mod port;
mod prune;
mod stats;
mod top;
mod unpause;
mod wait;

pub fn top(service: &ServiceConfig, top_args: &[String]) -> Result<()> {
    top::top(service, top_args)
}

pub fn stats(service: &ServiceConfig, no_stream: bool, format: Option<&str>) -> Result<()> {
    stats::stats(service, no_stream, format)
}

pub fn inspect_container(
    service: &ServiceConfig,
    format: Option<&str>,
    size: bool,
    object_type: Option<&str>,
) -> Result<()> {
    inspect::inspect_container(service, format, size, object_type)
}

pub fn attach(
    service: &ServiceConfig,
    no_stdin: bool,
    sig_proxy: bool,
    detach_keys: Option<&str>,
) -> Result<()> {
    attach::attach(service, no_stdin, sig_proxy, detach_keys)
}

pub fn cp(source: &str, destination: &str, follow_link: bool, archive: bool) -> Result<()> {
    cp::cp(source, destination, follow_link, archive)
}

pub fn kill(service: &ServiceConfig, signal: Option<&str>) -> Result<()> {
    kill::kill(service, signal)
}

pub fn pause(service: &ServiceConfig) -> Result<()> {
    pause::pause(service)
}

pub fn unpause(service: &ServiceConfig) -> Result<()> {
    unpause::unpause(service)
}

pub fn wait(service: &ServiceConfig, condition: Option<&str>) -> Result<()> {
    wait::wait(service, condition)
}

pub fn events(
    since: Option<&str>,
    until: Option<&str>,
    format: Option<&str>,
    filters: &[String],
) -> Result<()> {
    events::events(since, until, format, filters)
}

pub fn port(service: &ServiceConfig, private_port: Option<&str>) -> Result<()> {
    port::port(service, private_port)
}

pub fn port_output(service: &ServiceConfig, private_port: Option<&str>) -> Result<String> {
    port::port_output(service, private_port)
}

pub fn prune(force: bool, filters: &[String]) -> Result<()> {
    prune::prune(force, filters)
}

pub fn prune_stopped_container(service: &ServiceConfig) -> Result<()> {
    prune::prune_stopped_container(service)
}
