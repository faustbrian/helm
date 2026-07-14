use super::{V7LogicalDataInventory, V7RuntimeFeature, V7VolumeInventory};
use crate::config::{Driver, Kind};
use crate::control_plane::engine::ObservedContainerMount;
use std::collections::BTreeMap;

/// Complete secret-free fields for one legacy service inventory record.
pub(super) struct V7ServiceInventoryOptions {
    pub(super) service_id: String,
    pub(super) kind: Kind,
    pub(super) driver: Driver,
    pub(super) image_reference: String,
    pub(super) container_name: String,
    pub(super) observed_container_id: Option<String>,
    pub(super) observed_image_identity: Option<String>,
    pub(super) observed_mounts: Vec<ObservedContainerMount>,
    pub(super) volumes: Vec<V7VolumeInventory>,
    pub(super) logical_data: V7LogicalDataInventory,
    pub(super) credential_fields: Vec<String>,
    pub(super) environment_keys: Vec<String>,
    pub(super) environment_mapping: BTreeMap<String, String>,
    pub(super) runtime_features: Vec<V7RuntimeFeature>,
}
