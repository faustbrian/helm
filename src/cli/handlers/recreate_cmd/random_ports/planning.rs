//! recreate random-ports planning helpers.

use anyhow::Result;

use crate::{cli, config};

pub(super) fn plan_randomized_runtimes(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
) -> Result<(Vec<config::ServiceConfig>, config::Config)> {
    let selected = cli::support::selected_runtime_services(config, service, kind)?;
    let mut runtime_config = config.clone();
    let mut used_ports = cli::support::collect_service_host_ports(&runtime_config.service);
    let mut planned = Vec::new();

    for mut runtime in selected {
        let should_remap_smtp = runtime.driver == config::Driver::Mailhog;
        cli::support::remap_random_ports(&mut runtime, &mut used_ports, should_remap_smtp)?;
        cli::support::apply_runtime_binding(&mut runtime_config, &runtime)?;
        planned.push(runtime);
    }

    Ok((planned, runtime_config))
}
