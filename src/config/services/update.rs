use anyhow::Result;

use crate::config::Config;

pub(crate) fn update_service_port(config: &mut Config, name: &str, port: u16) -> Result<()> {
    let service = config
        .service
        .iter_mut()
        .find(|svc| svc.name == name)
        .ok_or_else(|| anyhow::anyhow!("service '{name}' not found while updating port"))?;
    service.port = port;
    Ok(())
}

pub(crate) fn update_service_host_port(
    config: &mut Config,
    name: &str,
    host: &str,
    port: u16,
) -> Result<bool> {
    let service = config
        .service
        .iter_mut()
        .find(|svc| svc.name == name)
        .ok_or_else(|| anyhow::anyhow!("service '{name}' not found while updating endpoint"))?;

    let changed = service.host != host || service.port != port;
    if changed {
        service.host = host.to_owned();
        service.port = port;
    }
    Ok(changed)
}
