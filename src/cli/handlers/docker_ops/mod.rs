//! cli handlers docker ops module.
//!
//! Contains docker passthrough command handlers used by Helm command workflows.

use anyhow::Result;

use crate::config;

mod attach;
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

pub(crate) fn handle_top(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    args: &[String],
) -> Result<()> {
    top::handle_top(config, service, kind, args)
}

pub(crate) fn handle_stats(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    no_stream: bool,
    format: Option<&str>,
) -> Result<()> {
    stats::handle_stats(config, service, kind, no_stream, format)
}

pub(crate) fn handle_inspect(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    format: Option<&str>,
    size: bool,
    object_type: Option<&str>,
) -> Result<()> {
    inspect::handle_inspect(config, service, kind, format, size, object_type)
}

pub(crate) fn handle_attach(
    config: &config::Config,
    service: Option<&str>,
    no_stdin: bool,
    sig_proxy: bool,
    detach_keys: Option<&str>,
) -> Result<()> {
    attach::handle_attach(config, service, no_stdin, sig_proxy, detach_keys)
}

pub(crate) fn handle_cp(
    config: &config::Config,
    source: &str,
    destination: &str,
    follow_link: bool,
    archive: bool,
) -> Result<()> {
    cp::handle_cp(config, source, destination, follow_link, archive)
}

pub(crate) fn handle_kill(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    signal: Option<&str>,
    parallel: usize,
) -> Result<()> {
    kill::handle_kill(config, service, kind, signal, parallel)
}

pub(crate) fn handle_pause(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
) -> Result<()> {
    pause::handle_pause(config, service, kind, parallel)
}

pub(crate) fn handle_unpause(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
) -> Result<()> {
    unpause::handle_unpause(config, service, kind, parallel)
}

pub(crate) fn handle_wait(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    condition: Option<&str>,
    parallel: usize,
) -> Result<()> {
    wait::handle_wait(config, service, kind, condition, parallel)
}

pub(crate) fn handle_events(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    since: Option<&str>,
    until: Option<&str>,
    format: Option<&str>,
    all: bool,
    filter: &[String],
) -> Result<()> {
    events::handle_events(config, service, kind, since, until, format, all, filter)
}

pub(crate) fn handle_port(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    private_port: Option<&str>,
) -> Result<()> {
    port::handle_port(config, service, kind, private_port)
}

pub(crate) fn handle_prune(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
    all: bool,
    force: bool,
    filter: &[String],
) -> Result<()> {
    prune::handle_prune(config, service, kind, parallel, all, force, filter)
}
