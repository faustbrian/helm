use serde::Deserialize;

/// The initial strict v8 service configuration boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
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
}

impl RawServiceConfig {
    /// Returns the exact compatibility version string when declared.
    pub(crate) fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// Returns exact declared service dependencies.
    pub(crate) fn depends_on(&self) -> &[String] {
        &self.depends_on
    }
}
