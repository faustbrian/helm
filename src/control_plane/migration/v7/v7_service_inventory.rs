use super::{
    V7LogicalDataInventory, V7RuntimeFeature, V7ServiceInventoryOptions, V7VolumeInventory,
};
use crate::config::{Driver, Kind};
use crate::control_plane::engine::ObservedContainerMount;
use std::collections::BTreeMap;

/// Secret-free legacy service contract bound to observed Engine identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7ServiceInventory {
    service_id: String,
    kind: Kind,
    driver: Driver,
    image_reference: String,
    container_name: String,
    observed_container_id: Option<String>,
    observed_image_identity: Option<String>,
    observed_mounts: Vec<ObservedContainerMount>,
    volumes: Vec<V7VolumeInventory>,
    logical_data: V7LogicalDataInventory,
    credential_fields: Vec<String>,
    environment_keys: Vec<String>,
    environment_mapping: BTreeMap<String, String>,
    runtime_features: Vec<V7RuntimeFeature>,
}

impl V7ServiceInventory {
    pub(super) fn new(options: V7ServiceInventoryOptions) -> Self {
        Self {
            service_id: options.service_id,
            kind: options.kind,
            driver: options.driver,
            image_reference: options.image_reference,
            container_name: options.container_name,
            observed_container_id: options.observed_container_id,
            observed_image_identity: options.observed_image_identity,
            observed_mounts: options.observed_mounts,
            volumes: options.volumes,
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

    pub(crate) const fn kind(&self) -> Kind {
        self.kind
    }

    pub(crate) const fn driver(&self) -> Driver {
        self.driver
    }

    pub(crate) fn image_reference(&self) -> &str {
        &self.image_reference
    }

    pub(crate) fn container_name(&self) -> &str {
        &self.container_name
    }

    pub(crate) fn observed_container_id(&self) -> Option<&str> {
        self.observed_container_id.as_deref()
    }

    pub(crate) fn observed_image_identity(&self) -> Option<&str> {
        self.observed_image_identity.as_deref()
    }

    pub(crate) fn observed_mounts(&self) -> &[ObservedContainerMount] {
        &self.observed_mounts
    }

    pub(crate) fn volumes(&self) -> &[V7VolumeInventory] {
        &self.volumes
    }

    pub(crate) const fn logical_data(&self) -> &V7LogicalDataInventory {
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

    pub(crate) fn runtime_features(&self) -> &[V7RuntimeFeature] {
        &self.runtime_features
    }
}
