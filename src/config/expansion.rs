use anyhow::Result;

use super::{Config, RawConfig, RawServiceConfig, ServiceConfig, SwarmTarget};

mod ports;
mod service;

pub(super) fn expand_raw_config(raw: RawConfig) -> Result<Config> {
    let schema_version = raw.schema_version.unwrap_or(1);
    if schema_version != 1 {
        anyhow::bail!("unsupported schema_version '{schema_version}'; run `helm config migrate`");
    }

    let mut services: Vec<ServiceConfig> = raw
        .service
        .into_iter()
        .map(expand_raw_service)
        .collect::<Result<Vec<_>>>()?;

    ports::assign_missing_ports(&mut services)?;

    Ok(Config {
        schema_version,
        container_prefix: raw.container_prefix,
        service: services,
        swarm: raw
            .swarm
            .into_iter()
            .map(|target| {
                let git = match target.git.len() {
                    0 => None,
                    1 => Some(super::SwarmGit {
                        repo: target.git[0].repo.clone(),
                        branch: target.git[0].branch.clone(),
                    }),
                    _ => {
                        anyhow::bail!(
                            "swarm target '{}' must define at most one git block",
                            target.name
                        );
                    }
                };

                Ok(SwarmTarget {
                    name: target.name,
                    root: target.root,
                    depends_on: target.depends_on,
                    inject_env: target.inject_env,
                    git,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

pub(super) fn expand_raw_service(raw: RawServiceConfig) -> Result<ServiceConfig> {
    service::expand_raw_service(raw)
}
