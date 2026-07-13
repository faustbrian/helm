use super::DesiredServiceOptions;
use crate::control_plane::{ServiceDeploymentStrategy, ServiceIdentity};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// One service after identity and dependency validation.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct DesiredService {
    identity: ServiceIdentity,
    dependencies: Vec<ServiceIdentity>,
    preset: Option<String>,
    deployment_strategy: Option<ServiceDeploymentStrategy>,
    image: Option<String>,
    version: Option<String>,
    php_extensions: Vec<String>,
    database: Option<String>,
    command: Option<Vec<String>>,
    environment: BTreeMap<String, String>,
}

impl Debug for DesiredService {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesiredService")
            .field("identity", &self.identity)
            .field("dependencies", &self.dependencies)
            .field("preset", &self.preset)
            .field("image", &self.image)
            .field("version", &self.version)
            .field("php_extensions", &self.php_extensions)
            .field("database", &self.database)
            .field("command", &self.command)
            .field("environment_keys", &self.environment.keys())
            .finish()
    }
}

impl DesiredService {
    pub(super) fn new(options: DesiredServiceOptions) -> Self {
        Self {
            identity: options.identity,
            dependencies: options.dependencies,
            preset: options.preset,
            deployment_strategy: options.deployment_strategy,
            image: options.image,
            version: options.version,
            php_extensions: options.php_extensions,
            database: options.database,
            command: options.command,
            environment: options.environment,
        }
    }

    /// Returns the exact validated service identity.
    pub(crate) fn name(&self) -> &str {
        self.identity.as_str()
    }

    /// Returns dependencies in deterministic identity order.
    pub(crate) fn dependencies(&self) -> &[ServiceIdentity] {
        &self.dependencies
    }

    pub(crate) fn preset(&self) -> Option<&str> {
        self.preset.as_deref()
    }

    /// Whether reconciliation for this service produces a gateway route.
    pub(crate) const fn claims_gateway_route(&self) -> bool {
        match self.deployment_strategy {
            Some(strategy) => strategy.claims_gateway_route(),
            None => self.image.is_some(),
        }
    }

    pub(crate) fn image(&self) -> Option<&str> {
        self.image.as_deref()
    }

    pub(crate) fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub(crate) fn php_extensions(&self) -> &[String] {
        &self.php_extensions
    }

    pub(crate) fn database(&self) -> Option<&str> {
        self.database.as_deref()
    }

    pub(crate) fn command(&self) -> Option<&[String]> {
        self.command.as_deref()
    }

    pub(crate) const fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    pub(super) const fn identity(&self) -> &ServiceIdentity {
        &self.identity
    }
}
