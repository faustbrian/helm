use super::{IpcV7Mount, IpcV7ServiceInventoryOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Complete secret-free legacy service evidence returned to the operator.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcV7ServiceInventory {
    service_id: String,
    kind: String,
    driver: String,
    configured_image: String,
    observed_image: Option<String>,
    container_name: String,
    observed_container_id: Option<String>,
    configured_mounts: Vec<IpcV7Mount>,
    observed_mounts: Vec<IpcV7Mount>,
    logical_data: BTreeMap<String, String>,
    credential_fields: Vec<String>,
    environment_keys: Vec<String>,
    environment_mapping: BTreeMap<String, String>,
    runtime_features: Vec<String>,
}

impl IpcV7ServiceInventory {
    pub(crate) fn new(options: IpcV7ServiceInventoryOptions) -> Self {
        Self {
            service_id: options.service_id,
            kind: options.kind,
            driver: options.driver,
            configured_image: options.configured_image,
            observed_image: options.observed_image,
            container_name: options.container_name,
            observed_container_id: options.observed_container_id,
            configured_mounts: options.configured_mounts,
            observed_mounts: options.observed_mounts,
            logical_data: options.logical_data,
            credential_fields: options.credential_fields,
            environment_keys: options.environment_keys,
            environment_mapping: options.environment_mapping,
            runtime_features: options.runtime_features,
        }
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn driver(&self) -> &str {
        &self.driver
    }

    pub(crate) fn configured_image(&self) -> &str {
        &self.configured_image
    }

    pub(crate) fn observed_image(&self) -> Option<&str> {
        self.observed_image.as_deref()
    }

    pub(crate) fn container_name(&self) -> &str {
        &self.container_name
    }

    pub(crate) fn observed_container_id(&self) -> Option<&str> {
        self.observed_container_id.as_deref()
    }

    pub(crate) fn configured_mounts(&self) -> &[IpcV7Mount] {
        &self.configured_mounts
    }

    pub(crate) fn observed_mounts(&self) -> &[IpcV7Mount] {
        &self.observed_mounts
    }

    pub(crate) fn logical_data(&self) -> &BTreeMap<String, String> {
        &self.logical_data
    }

    pub(crate) fn credential_fields(&self) -> &[String] {
        &self.credential_fields
    }

    pub(crate) fn environment_keys(&self) -> &[String] {
        &self.environment_keys
    }

    pub(crate) fn environment_mapping(&self) -> &BTreeMap<String, String> {
        &self.environment_mapping
    }

    pub(crate) fn runtime_features(&self) -> &[String] {
        &self.runtime_features
    }
}
