use crate::control_plane::{ServiceDeploymentStrategy, ServiceIdentity};
use std::collections::BTreeMap;

/// Complete validated declarative fields for one desired v8 service.
pub(crate) struct DesiredServiceOptions {
    pub(crate) identity: ServiceIdentity,
    pub(crate) dependencies: Vec<ServiceIdentity>,
    pub(crate) preset: Option<String>,
    pub(crate) deployment_strategy: Option<ServiceDeploymentStrategy>,
    pub(crate) image: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) php_extensions: Vec<String>,
    pub(crate) database: Option<String>,
    pub(crate) command: Option<Vec<String>>,
    pub(crate) environment: BTreeMap<String, String>,
}
