use anyhow::Result;

use crate::{cli, config, docker};

pub(crate) fn handle_logs(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    all: bool,
    follow: bool,
    tail: Option<u64>,
    timestamps: bool,
    prefix: bool,
    access: bool,
) -> Result<()> {
    if access {
        return cli::support::tail_access_logs(follow, tail);
    }

    let filtered = cli::support::filter_services(&config.service, kind, None);
    if all {
        let cloned: Vec<config::ServiceConfig> = filtered.iter().copied().cloned().collect();
        return docker::logs_many(&cloned, follow, tail, timestamps, prefix);
    }

    let svc = if let Some(name) = service {
        config::find_service(config, name)?
    } else {
        config::resolve_service(config, None)?
    };
    if prefix {
        docker::logs_prefixed(svc, follow, tail, timestamps)
    } else {
        docker::logs(svc, follow, tail, timestamps)
    }
}
