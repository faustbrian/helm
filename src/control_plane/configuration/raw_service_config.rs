use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// The initial strict v8 service configuration boundary.
#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawServiceConfig {
    preset: Option<String>,
    image: Option<String>,
    version: Option<String>,
    #[serde(default)]
    php_extensions: Vec<String>,
    #[serde(default)]
    depends_on: Vec<String>,
    database: Option<String>,
    command: Option<Vec<String>>,
    #[serde(default)]
    environment: BTreeMap<String, String>,
}

impl Debug for RawServiceConfig {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RawServiceConfig")
            .field("preset", &self.preset)
            .field("image", &self.image)
            .field("version", &self.version)
            .field("php_extensions", &self.php_extensions)
            .field("depends_on", &self.depends_on)
            .field("database", &self.database)
            .field("command", &self.command)
            .field("environment_keys", &self.environment.keys())
            .finish()
    }
}

impl RawServiceConfig {
    /// Returns the exact declared preset.
    pub(crate) fn preset(&self) -> Option<&str> {
        self.preset.as_deref()
    }

    /// Returns the exact declared image reference before immutable resolution.
    pub(crate) fn image(&self) -> Option<&str> {
        self.image.as_deref()
    }

    pub(super) fn set_image(&mut self, image: String) {
        self.image = Some(image);
    }

    /// Returns the exact compatibility version string when declared.
    pub(crate) fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// Returns exact declared service dependencies.
    pub(crate) fn depends_on(&self) -> &[String] {
        &self.depends_on
    }

    /// Returns exact declared PHP extensions.
    pub(crate) fn php_extensions(&self) -> &[String] {
        &self.php_extensions
    }

    /// Returns the exact requested logical database name.
    pub(crate) fn database(&self) -> Option<&str> {
        self.database.as_deref()
    }

    /// Returns the exact requested process command when declared.
    pub(crate) fn command(&self) -> Option<&[String]> {
        self.command.as_deref()
    }

    /// Returns exact project-visible process environment values.
    pub(crate) const fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }
}
