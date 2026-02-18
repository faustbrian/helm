//! cli handlers docker ops inspect module.
//!
//! Contains inspect handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

use super::output_json::{collect_service_json, print_pretty_json};

pub(crate) struct HandleInspectOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) format: Option<&'a str>,
    pub(crate) json: bool,
    pub(crate) size: bool,
    pub(crate) object_type: Option<&'a str>,
}

pub(crate) fn handle_inspect(
    config: &config::Config,
    options: HandleInspectOptions<'_>,
) -> Result<()> {
    let selected = super::selected_docker_services(config, options.service, options.kind)?;
    if options.json {
        let items = collect_service_json(selected, |service| {
            let container_name = service.container_name()?;
            let payload = docker::inspect_json(&container_name)
                .ok_or_else(|| anyhow::anyhow!("Failed to inspect container '{container_name}'"))?;
            Ok(vec![payload])
        })?;
        return print_pretty_json(&items);
    }

    super::run_for_selected_docker_services(config, options.service, options.kind, |svc| {
        docker::inspect_container(svc, options.format, options.size, options.object_type)
    })
}
