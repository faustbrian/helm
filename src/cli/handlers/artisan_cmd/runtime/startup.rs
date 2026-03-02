//! Test runtime startup helpers for artisan flows.

use anyhow::Result;

use crate::{cli, config, docker};

const TEST_HEALTH_TIMEOUT_SECS: u64 = 60;

pub(super) fn resolve_testing_startup_services(
    config: &config::Config,
) -> Result<Vec<&config::ServiceConfig>> {
    cli::support::resolve_up_services(config, None, None, None)
}

pub(super) fn run_testing_startup_services<F>(
    startup_services: &[&config::ServiceConfig],
    start_context: &cli::support::ServiceStartContext<'_>,
    reset_service_runtime: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig) -> Result<()>,
{
    for svc in startup_services {
        let wait_healthy = svc.kind != config::Kind::App;
        reset_service_runtime(svc)?;
        cli::support::start_service(
            svc,
            start_context,
            false,
            docker::PullPolicy::Missing,
            wait_healthy,
            TEST_HEALTH_TIMEOUT_SECS,
            true,
        )?;
    }

    Ok(())
}
