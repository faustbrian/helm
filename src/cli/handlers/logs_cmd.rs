//! cli handlers logs cmd module.
//!
//! Contains cli handlers logs cmd logic used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(crate) struct HandleLogsOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) all: bool,
    pub(crate) follow: bool,
    pub(crate) tail: Option<u64>,
    pub(crate) since: Option<&'a str>,
    pub(crate) until: Option<&'a str>,
    pub(crate) timestamps: bool,
    pub(crate) prefix: bool,
    pub(crate) access: bool,
}

pub(crate) fn handle_logs(config: &config::Config, options: HandleLogsOptions<'_>) -> Result<()> {
    if options.access {
        return cli::support::tail_access_logs(options.follow, options.tail);
    }

    let filtered = cli::support::filter_services(&config.service, options.kind, None);
    if options.all {
        let cloned: Vec<config::ServiceConfig> = filtered.iter().copied().cloned().collect();
        return docker::logs_many(
            &cloned,
            docker_logs_options(
                options.follow,
                options.tail,
                options.since,
                options.until,
                options.timestamps,
                options.prefix,
            ),
        );
    }

    let svc = if !options.services.is_empty() {
        let cloned: Vec<config::ServiceConfig> = options
            .services
            .iter()
            .map(|name| config::find_service(config, name).cloned())
            .collect::<Result<_, _>>()?;
        return docker::logs_many(
            &cloned,
            docker_logs_options(
                options.follow,
                options.tail,
                options.since,
                options.until,
                options.timestamps,
                options.prefix,
            ),
        );
    } else if let Some(name) = options.service {
        config::find_service(config, name)?
    } else {
        config::resolve_service(config, None)?
    };
    run_single_service_logs(
        svc,
        options.follow,
        options.tail,
        options.since,
        options.until,
        options.timestamps,
        options.prefix,
    )
}

fn run_single_service_logs(
    service: &config::ServiceConfig,
    follow: bool,
    tail: Option<u64>,
    since: Option<&str>,
    until: Option<&str>,
    timestamps: bool,
    prefix: bool,
) -> Result<()> {
    let log_options = docker_logs_options(follow, tail, since, until, timestamps, prefix);
    if prefix {
        docker::logs_prefixed(service, log_options)
    } else {
        docker::logs(service, log_options)
    }
}

fn docker_logs_options(
    follow: bool,
    tail: Option<u64>,
    since: Option<&str>,
    until: Option<&str>,
    timestamps: bool,
    prefix: bool,
) -> docker::LogsOptions {
    docker::LogsOptions {
        follow,
        tail,
        since: since.map(ToOwned::to_owned),
        until: until.map(ToOwned::to_owned),
        timestamps,
        prefix,
    }
}
