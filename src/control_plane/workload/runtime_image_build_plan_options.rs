use super::JavaScriptRuntimeSpec;

/// Complete immutable inputs for one reusable application runtime image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeImageBuildPlanOptions {
    pub(crate) installation_id: String,
    pub(crate) schema_version: u32,
    pub(crate) base_image_digest: String,
    pub(crate) platform: String,
    pub(crate) php_version: String,
    pub(crate) php_extensions: Vec<String>,
    pub(crate) system_packages: Vec<String>,
    pub(crate) composer_version: String,
    pub(crate) javascript: Option<JavaScriptRuntimeSpec>,
    pub(crate) installer_revision: String,
    pub(crate) installer_sha256: String,
    pub(crate) installer: Vec<u8>,
}
