use super::{
    IpcV7HostArtifactInventory, IpcV7ProjectInventoryOptions, IpcV7Route, IpcV7ServiceInventory,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Complete secret-free v7 source inventory returned before migration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcV7ProjectInventory {
    project_id: String,
    canonical_project_path: PathBuf,
    source_revision: String,
    schema_version: u32,
    services: Vec<IpcV7ServiceInventory>,
    routes: Vec<IpcV7Route>,
    blockers: Vec<String>,
    requires_legacy_ca_capture: bool,
    #[serde(default)]
    host_artifacts: IpcV7HostArtifactInventory,
}

impl From<&crate::control_plane::migration::V7ProjectInventory> for IpcV7ProjectInventory {
    fn from(inventory: &crate::control_plane::migration::V7ProjectInventory) -> Self {
        Self::new(IpcV7ProjectInventoryOptions {
            project_id: inventory.project_id().to_owned(),
            canonical_project_path: inventory.canonical_project_path().to_path_buf(),
            source_revision: inventory.source_revision().to_owned(),
            schema_version: inventory.schema_version(),
            services: inventory.services().iter().map(service).collect(),
            routes: inventory
                .routes()
                .iter()
                .map(|route| {
                    IpcV7Route::new(
                        route.service_id().to_owned(),
                        route.domain().to_owned(),
                        route.scheme().to_owned(),
                        route.host_port(),
                    )
                })
                .collect(),
            blockers: inventory
                .blockers()
                .iter()
                .map(ToString::to_string)
                .collect(),
            requires_legacy_ca_capture: inventory.requires_legacy_ca_capture(),
        })
        .with_host_artifacts(IpcV7HostArtifactInventory::from(inventory.host_artifacts()))
    }
}

fn service(service: &crate::control_plane::migration::V7ServiceInventory) -> IpcV7ServiceInventory {
    let mut logical_data = BTreeMap::new();
    if let Some(database) = service.logical_database() {
        logical_data.insert("database".to_owned(), database.to_owned());
    }
    if let Some(bucket) = service.logical_bucket() {
        logical_data.insert("bucket".to_owned(), bucket.to_owned());
    }
    if let Some(region) = service.logical_region() {
        logical_data.insert("region".to_owned(), region.to_owned());
    }
    IpcV7ServiceInventory::new(super::IpcV7ServiceInventoryOptions {
        service_id: service.service_id().to_owned(),
        kind: kind_label(service.kind()).to_owned(),
        driver: driver_label(service.driver()).to_owned(),
        configured_image: service.image_reference().to_owned(),
        observed_image: service.observed_image_identity().map(str::to_owned),
        container_name: service.container_name().to_owned(),
        observed_container_id: service.observed_container_id().map(str::to_owned),
        configured_mounts: service.volumes().iter().map(configured_mount).collect(),
        observed_mounts: service
            .observed_mounts()
            .iter()
            .map(|mount| {
                super::IpcV7Mount::new(
                    if mount.is_named_volume() {
                        "named_volume"
                    } else {
                        "host_or_runtime"
                    }
                    .to_owned(),
                    mount.source().to_owned(),
                    mount.target().to_owned(),
                    mount.is_read_only(),
                )
            })
            .collect(),
        logical_data,
        credential_fields: service.credential_fields().to_vec(),
        environment_keys: service.environment_keys().to_vec(),
        environment_mapping: service.environment_mapping().clone(),
        runtime_features: service
            .runtime_features()
            .iter()
            .map(|feature| feature.label().to_owned())
            .collect(),
    })
}

fn configured_mount(
    mount: &crate::control_plane::migration::V7VolumeInventory,
) -> super::IpcV7Mount {
    let (source_kind, source) = match mount.source() {
        crate::control_plane::migration::V7VolumeSource::Named(source) => {
            ("named_volume", source.as_str())
        }
        crate::control_plane::migration::V7VolumeSource::HostBind(source) => {
            ("host_bind", source.as_str())
        }
        crate::control_plane::migration::V7VolumeSource::Anonymous => ("anonymous", "<anonymous>"),
        crate::control_plane::migration::V7VolumeSource::Unsupported => {
            ("unsupported", "<unsupported>")
        }
    };
    super::IpcV7Mount::new(
        source_kind.to_owned(),
        source.to_owned(),
        mount.target().to_owned(),
        mount.is_read_only(),
    )
}

const fn kind_label(kind: crate::config::Kind) -> &'static str {
    match kind {
        crate::config::Kind::Database => "database",
        crate::config::Kind::Cache => "cache",
        crate::config::Kind::ObjectStore => "object_store",
        crate::config::Kind::Search => "search",
        crate::config::Kind::App => "app",
    }
}

const fn driver_label(driver: crate::config::Driver) -> &'static str {
    match driver {
        crate::config::Driver::Mongodb => "mongodb",
        crate::config::Driver::Memcached => "memcached",
        crate::config::Driver::Postgres => "postgres",
        crate::config::Driver::Mysql => "mysql",
        crate::config::Driver::Sqlserver => "sqlserver",
        crate::config::Driver::Redis => "redis",
        crate::config::Driver::Valkey => "valkey",
        crate::config::Driver::Dragonfly => "dragonfly",
        crate::config::Driver::Minio => "minio",
        crate::config::Driver::Garage => "garage",
        crate::config::Driver::Rustfs => "rustfs",
        crate::config::Driver::Localstack => "localstack",
        crate::config::Driver::Opensearch => "opensearch",
        crate::config::Driver::Elasticsearch => "elasticsearch",
        crate::config::Driver::Meilisearch => "meilisearch",
        crate::config::Driver::Typesense => "typesense",
        crate::config::Driver::Frankenphp => "frankenphp",
        crate::config::Driver::Reverb => "reverb",
        crate::config::Driver::Horizon => "horizon",
        crate::config::Driver::Scheduler => "scheduler",
        crate::config::Driver::Dusk => "dusk",
        crate::config::Driver::Gotenberg => "gotenberg",
        crate::config::Driver::Mailhog => "mailhog",
        crate::config::Driver::Rabbitmq => "rabbitmq",
        crate::config::Driver::Soketi => "soketi",
    }
}

impl IpcV7ProjectInventory {
    pub(crate) fn new(options: IpcV7ProjectInventoryOptions) -> Self {
        Self {
            project_id: options.project_id,
            canonical_project_path: options.canonical_project_path,
            source_revision: options.source_revision,
            schema_version: options.schema_version,
            services: options.services,
            routes: options.routes,
            blockers: options.blockers,
            requires_legacy_ca_capture: options.requires_legacy_ca_capture,
            host_artifacts: IpcV7HostArtifactInventory::default(),
        }
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn canonical_project_path(&self) -> &Path {
        &self.canonical_project_path
    }

    pub(crate) fn source_revision(&self) -> &str {
        &self.source_revision
    }

    pub(crate) const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub(crate) fn services(&self) -> &[IpcV7ServiceInventory] {
        &self.services
    }

    pub(crate) fn routes(&self) -> &[IpcV7Route] {
        &self.routes
    }

    pub(crate) fn blockers(&self) -> &[String] {
        &self.blockers
    }

    pub(crate) const fn requires_legacy_ca_capture(&self) -> bool {
        self.requires_legacy_ca_capture
    }

    pub(crate) fn ready_for_automatic_migration(&self) -> bool {
        self.blockers.is_empty()
    }

    pub(crate) fn with_host_artifacts(
        mut self,
        host_artifacts: IpcV7HostArtifactInventory,
    ) -> Self {
        self.host_artifacts = host_artifacts;
        self
    }

    pub(crate) const fn host_artifacts(&self) -> &IpcV7HostArtifactInventory {
        &self.host_artifacts
    }
}
