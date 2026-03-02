//! config service methods identity module.
//!
//! Contains config service methods identity logic used by Helm command workflows.

use anyhow::{Result, anyhow};

use super::ServiceConfig;

impl ServiceConfig {
    /// Returns the Docker container name for this service.
    #[must_use]
    pub fn container_name(&self) -> Result<String> {
        self.resolved_container_name
            .clone()
            .or_else(|| self.container_name.clone())
            .ok_or_else(|| anyhow!("service '{}' has no resolved container name", self.name))
    }

    #[must_use]
    pub fn scheme(&self) -> &str {
        self.scheme.as_deref().unwrap_or("http")
    }
}
