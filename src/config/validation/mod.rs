use anyhow::Result;

use super::Config;

mod container_names;
mod swarm;

pub(super) fn validate_and_resolve_container_names(config: &mut Config) -> Result<()> {
    container_names::validate_and_resolve_container_names(config)
}

pub(super) fn validate_swarm_targets(config: &Config) -> Result<()> {
    swarm::validate_swarm_targets(config)
}
