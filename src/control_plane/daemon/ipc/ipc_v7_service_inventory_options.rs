use super::IpcV7Mount;
use std::collections::BTreeMap;

/// Complete secret-free wire fields for one legacy service.
pub(crate) struct IpcV7ServiceInventoryOptions {
    pub(crate) service_id: String,
    pub(crate) kind: String,
    pub(crate) driver: String,
    pub(crate) configured_image: String,
    pub(crate) observed_image: Option<String>,
    pub(crate) container_name: String,
    pub(crate) observed_container_id: Option<String>,
    pub(crate) configured_mounts: Vec<IpcV7Mount>,
    pub(crate) observed_mounts: Vec<IpcV7Mount>,
    pub(crate) logical_data: BTreeMap<String, String>,
    pub(crate) credential_fields: Vec<String>,
    pub(crate) environment_keys: Vec<String>,
    pub(crate) environment_mapping: BTreeMap<String, String>,
    pub(crate) runtime_features: Vec<String>,
}
