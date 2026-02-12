use serde::{Deserialize, Serialize};

use super::{ServiceConfig, SwarmTarget};

/// Root configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Config {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Global container name prefix used when service-level name is not set.
    pub container_prefix: Option<String>,
    /// List of service configurations.
    #[serde(default)]
    pub service: Vec<ServiceConfig>,
    /// Optional swarm workspace targets.
    #[serde(default)]
    pub swarm: Vec<SwarmTarget>,
}

const fn default_schema_version() -> u32 {
    1
}
