//! config types lockfile module.
//!
//! Contains config types lockfile logic used by Helm command workflows.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub images: Vec<LockedImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedImage {
    pub service: String,
    pub image: String,
    pub resolved: String,
}

/// Returns the default value for version.
const fn default_version() -> u32 {
    1
}
