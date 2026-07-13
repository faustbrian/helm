use super::DesiredServiceOptions;
use crate::control_plane::ServiceIdentity;

/// One service after identity and dependency validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesiredService {
    identity: ServiceIdentity,
    dependencies: Vec<ServiceIdentity>,
    preset: Option<String>,
    image: Option<String>,
    version: Option<String>,
    php_extensions: Vec<String>,
    database: Option<String>,
}

impl DesiredService {
    pub(super) fn new(options: DesiredServiceOptions) -> Self {
        Self {
            identity: options.identity,
            dependencies: options.dependencies,
            preset: options.preset,
            image: options.image,
            version: options.version,
            php_extensions: options.php_extensions,
            database: options.database,
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

    pub(super) const fn identity(&self) -> &ServiceIdentity {
        &self.identity
    }
}
