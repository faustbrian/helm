use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

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
