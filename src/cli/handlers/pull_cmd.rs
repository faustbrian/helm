use anyhow::Result;

use crate::{cli, config, docker};

pub(crate) fn handle_pull(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
) -> Result<()> {
    cli::support::for_each_service(config, service, kind, None, parallel, docker::pull)
}
