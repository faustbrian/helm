use crate::control_plane::ServiceIdentity;
use std::collections::BTreeMap;

/// Complete validated declarative fields for one desired v8 service.
pub(crate) struct DesiredServiceOptions {
    pub(crate) identity: ServiceIdentity,
    pub(crate) dependencies: Vec<ServiceIdentity>,
    pub(crate) preset: Option<String>,
    pub(crate) image: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) php_extensions: Vec<String>,
    pub(crate) composer_image: Option<String>,
    pub(crate) node_image: Option<String>,
    pub(crate) bun_image: Option<String>,
    pub(crate) database: Option<String>,
    pub(crate) command: Option<Vec<String>>,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) environment_mapping: BTreeMap<String, String>,
}
