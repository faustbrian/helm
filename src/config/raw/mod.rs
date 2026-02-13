//! config raw module.
//!
//! Contains config raw logic used by Helm command workflows.

use serde::Deserialize;

mod service;
mod swarm;
mod swarm_git;

pub(crate) use service::RawServiceConfig;
pub(crate) use swarm::RawSwarmTarget;
pub(crate) use swarm_git::RawSwarmGit;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawConfig {
    #[serde(default)]
    pub schema_version: Option<u32>,
    pub container_prefix: Option<String>,
    #[serde(default)]
    pub service: Vec<RawServiceConfig>,
    #[serde(default)]
    pub swarm: Vec<RawSwarmTarget>,
}
