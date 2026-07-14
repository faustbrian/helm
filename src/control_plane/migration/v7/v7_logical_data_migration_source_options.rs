use std::collections::BTreeMap;

/// Complete accepted identity for one legacy logical-data provider source.
pub(crate) struct V7LogicalDataMigrationSourceOptions {
    pub(crate) project_id: String,
    pub(crate) service_id: String,
    pub(crate) kind: String,
    pub(crate) driver: String,
    pub(crate) container_name: String,
    pub(crate) container_id: String,
    pub(crate) named_volumes: Vec<String>,
    pub(crate) logical_data: BTreeMap<String, String>,
}
