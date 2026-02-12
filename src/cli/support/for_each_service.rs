use anyhow::Result;
use rayon::prelude::*;

use crate::config;

use super::filter_services::filter_services;
use super::matches_filter::matches_filter;

pub(crate) fn for_each_service(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
    parallel: usize,
    f: impl Fn(&config::ServiceConfig) -> Result<()> + Sync + Send,
) -> Result<()> {
    if parallel == 0 {
        anyhow::bail!("--parallel must be >= 1");
    }

    if let Some(name) = service {
        let svc = config::find_service(config, name)?;
        if !matches_filter(svc, kind, driver) {
            return Ok(());
        }
        return f(svc);
    }

    let selected: Vec<&config::ServiceConfig> = filter_services(&config.service, kind, driver);

    if parallel <= 1 {
        for svc in selected {
            f(svc)?;
        }
        return Ok(());
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(parallel)
        .build()?
        .install(|| selected.par_iter().try_for_each(|svc| f(svc)))
}
