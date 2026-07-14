use super::{
    V7HostArtifactInventory, V7InventoryBlocker, V7InventoryError, V7LogicalDataInventory,
    V7ProjectInventoryOptions, V7RouteInventory, V7RuntimeFeature, V7ServiceInventory,
    V7ServiceInventoryOptions, V7VolumeInventory,
};
use crate::config::{Driver, Kind, ServiceConfig};
use crate::control_plane::engine::ObservedContainer;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const LABEL_MANAGED: &str = "com.stackctl.managed";
const LABEL_CONTAINER: &str = "com.stackctl.container";
const LABEL_SERVICE: &str = "com.stackctl.service";
const LABEL_KIND: &str = "com.stackctl.kind";

/// Deterministic, secret-free v7 source inventory prepared before migration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7ProjectInventory {
    project_id: String,
    canonical_project_path: PathBuf,
    source_revision: String,
    schema_version: u32,
    services: Vec<V7ServiceInventory>,
    routes: Vec<V7RouteInventory>,
    blockers: Vec<V7InventoryBlocker>,
    requires_legacy_ca_capture: bool,
    host_artifacts: V7HostArtifactInventory,
}

impl V7ProjectInventory {
    pub(crate) fn new(options: V7ProjectInventoryOptions<'_>) -> Result<Self, V7InventoryError> {
        validate_options(&options)?;
        let mut configured = options.config.service.iter().collect::<Vec<_>>();
        configured.sort_by_key(|service| service.name.as_str());
        reject_duplicate_services(&configured)?;
        reject_duplicate_container_names(&configured)?;

        let mut blockers = Vec::new();
        if !options.config.swarm.is_empty() {
            blockers.push(V7InventoryBlocker::SwarmTargets {
                count: options.config.swarm.len(),
            });
        }
        let mut services = Vec::with_capacity(configured.len());
        let mut routes = Vec::new();
        let mut requires_legacy_ca_capture = false;
        for service in configured {
            let container_name = service.resolved_container_name.as_deref().ok_or_else(|| {
                V7InventoryError::new(format!(
                    "v7 service '{}' has no resolved container name",
                    service.name
                ))
            })?;
            let observed =
                match_observed_container(service, container_name, options.observed_containers)?;
            if observed.is_none() {
                blockers.push(V7InventoryBlocker::MissingContainer {
                    service_id: service.name.clone(),
                    container_name: container_name.to_owned(),
                });
            } else if observed
                .as_ref()
                .is_some_and(|(_, image_identity, _)| image_identity.is_none())
            {
                blockers.push(V7InventoryBlocker::MissingImageIdentity {
                    service_id: service.name.clone(),
                    container_name: container_name.to_owned(),
                });
            }
            let (volumes, volume_blockers) = volumes(service, container_name);
            blockers.extend(volume_blockers);
            if let Some((_, _, observed_mounts)) = &observed {
                blockers.extend(volume_observation_blockers(
                    &service.name,
                    &volumes,
                    observed_mounts,
                ));
            }
            routes.extend(service_routes(service));
            requires_legacy_ca_capture |= service.trust_container_ca;
            services.push(service_inventory(
                service,
                container_name,
                observed,
                volumes,
            ));
        }
        routes.sort();
        routes.dedup();
        blockers.sort();
        blockers.dedup();

        Ok(Self {
            project_id: options.project_id.to_owned(),
            canonical_project_path: options.canonical_project_path.to_path_buf(),
            source_revision: options.source_revision.to_owned(),
            schema_version: options.config.schema_version,
            services,
            routes,
            blockers,
            requires_legacy_ca_capture,
            host_artifacts: V7HostArtifactInventory::default(),
        })
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

    pub(crate) fn services(&self) -> &[V7ServiceInventory] {
        &self.services
    }

    pub(crate) fn routes(&self) -> &[V7RouteInventory] {
        &self.routes
    }

    pub(crate) fn blockers(&self) -> &[V7InventoryBlocker] {
        &self.blockers
    }

    pub(crate) const fn requires_legacy_ca_capture(&self) -> bool {
        self.requires_legacy_ca_capture
    }

    pub(crate) fn ready_for_automatic_migration(&self) -> bool {
        self.blockers.is_empty()
    }

    pub(crate) fn with_host_artifacts(mut self, host_artifacts: V7HostArtifactInventory) -> Self {
        self.host_artifacts = host_artifacts;
        self
    }

    pub(crate) const fn host_artifacts(&self) -> &V7HostArtifactInventory {
        &self.host_artifacts
    }
}

fn validate_options(options: &V7ProjectInventoryOptions<'_>) -> Result<(), V7InventoryError> {
    let revision = options
        .source_revision
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let valid_revision = revision.len() == 64
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if options.project_id.is_empty()
        || !options.canonical_project_path.is_absolute()
        || !valid_revision
        || options.config.schema_version == 0
        || options.config.schema_version > 7
    {
        return Err(V7InventoryError::new(
            "v7 inventory requires an absolute project path, exact source revision, and legacy schema",
        ));
    }
    Ok(())
}

fn reject_duplicate_services(services: &[&ServiceConfig]) -> Result<(), V7InventoryError> {
    for pair in services.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(V7InventoryError::new(format!(
                "v7 config contains duplicate service '{}'",
                pair[0].name
            )));
        }
    }
    Ok(())
}

fn reject_duplicate_container_names(services: &[&ServiceConfig]) -> Result<(), V7InventoryError> {
    let mut owners = BTreeMap::<&str, Vec<&str>>::new();
    for service in services {
        if let Some(name) = service.resolved_container_name.as_deref() {
            owners.entry(name).or_default().push(&service.name);
        }
    }
    if let Some((name, services)) = owners.iter().find(|(_, services)| services.len() > 1) {
        return Err(V7InventoryError::new(format!(
            "v7 container '{name}' is claimed by services: {}",
            services.join(", ")
        )));
    }
    Ok(())
}

fn match_observed_container(
    service: &ServiceConfig,
    container_name: &str,
    observed: &[ObservedContainer],
) -> Result<
    Option<(
        String,
        Option<String>,
        Vec<crate::control_plane::engine::ObservedContainerMount>,
    )>,
    V7InventoryError,
> {
    let mut matches = observed
        .iter()
        .filter(|container| {
            container.labels().get(LABEL_CONTAINER).map(String::as_str) == Some(container_name)
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|container| container.id().as_str());
    if matches.len() > 1 {
        return Err(V7InventoryError::new(format!(
            "v7 service '{}' container '{}' matches multiple Engine resources: {}",
            service.name,
            container_name,
            matches
                .iter()
                .map(|container| container.id().as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let Some(container) = matches.first() else {
        return Ok(None);
    };
    let labels = container.labels();
    let exact = labels.get(LABEL_MANAGED).map(String::as_str) == Some("true")
        && labels.get(LABEL_SERVICE).map(String::as_str) == Some(service.name.as_str())
        && labels.get(LABEL_KIND).map(String::as_str) == Some(kind_label(service.kind));
    if !exact {
        return Err(V7InventoryError::new(format!(
            "v7 service '{}' container '{}' has conflicting legacy ownership labels",
            service.name, container_name
        )));
    }

    Ok(Some((
        container.id().as_str().to_owned(),
        container.image_identity().map(str::to_owned),
        container.mounts().to_vec(),
    )))
}

fn service_inventory(
    service: &ServiceConfig,
    container_name: &str,
    observed: Option<(
        String,
        Option<String>,
        Vec<crate::control_plane::engine::ObservedContainerMount>,
    )>,
    volumes: Vec<V7VolumeInventory>,
) -> V7ServiceInventory {
    let credential_fields = [
        ("access_key", service.access_key.is_some()),
        ("api_key", service.api_key.is_some()),
        ("password", service.password.is_some()),
        ("secret_key", service.secret_key.is_some()),
        ("username", service.username.is_some()),
    ]
    .into_iter()
    .filter_map(|(field, present)| present.then(|| field.to_owned()))
    .collect();
    let environment_keys = service
        .env
        .as_ref()
        .map(|values| values.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default()
        .into_iter()
        .collect();
    let environment_mapping = service
        .env_mapping
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let runtime_features = runtime_features(service);

    let (observed_container_id, observed_image_identity, observed_mounts) = observed
        .map(|(container_id, image_identity, mounts)| (Some(container_id), image_identity, mounts))
        .unwrap_or((None, None, Vec::new()));
    V7ServiceInventory::new(V7ServiceInventoryOptions {
        service_id: service.name.clone(),
        kind: service.kind,
        driver: service.driver,
        image_reference: service.image.clone(),
        container_name: container_name.to_owned(),
        observed_container_id,
        observed_image_identity,
        observed_mounts,
        volumes,
        logical_data: V7LogicalDataInventory::from_service(service),
        credential_fields,
        environment_keys,
        environment_mapping,
        runtime_features,
    })
}

fn runtime_features(service: &ServiceConfig) -> Vec<V7RuntimeFeature> {
    let mut features = Vec::new();
    if !service.hook.is_empty() {
        features.push(V7RuntimeFeature::Hooks);
    }
    if service
        .php_extensions
        .as_ref()
        .is_some_and(|values| !values.is_empty())
    {
        features.push(V7RuntimeFeature::PhpExtensions);
    }
    if service.command.is_some() {
        features.push(V7RuntimeFeature::CustomCommand);
    }
    if service
        .env
        .as_ref()
        .is_some_and(|values| !values.is_empty())
    {
        features.push(V7RuntimeFeature::CustomEnvironment);
    }
    if service
        .env_mapping
        .as_ref()
        .is_some_and(|values| !values.is_empty())
    {
        features.push(V7RuntimeFeature::EnvironmentMapping);
    }
    if service.health_path.is_some() || service.health_statuses.is_some() {
        features.push(V7RuntimeFeature::HealthCheck);
    }
    if service.javascript.is_some() {
        features.push(V7RuntimeFeature::JavaScript);
    }
    if service.localhost_tls {
        features.push(V7RuntimeFeature::LocalhostTls);
    }
    if service.octane || service.octane_workers.is_some() || service.octane_max_requests.is_some() {
        features.push(V7RuntimeFeature::Octane);
    }
    if service.seed_file.is_some() {
        features.push(V7RuntimeFeature::SeedFile);
    }
    if service.restart.is_some() {
        features.push(V7RuntimeFeature::RestartPolicy);
    }
    features.sort();
    features
}

fn volumes(
    service: &ServiceConfig,
    container_name: &str,
) -> (Vec<V7VolumeInventory>, Vec<V7InventoryBlocker>) {
    let Some(explicit) = &service.volumes else {
        let volumes = default_data_target(service.driver)
            .map(|target| V7VolumeInventory::named(format!("{container_name}-data"), target))
            .into_iter()
            .collect();
        return (volumes, Vec::new());
    };
    let mut volumes = Vec::new();
    let mut blockers = Vec::new();
    for mount in explicit {
        let (volume, blocker) = V7VolumeInventory::from_mount(&service.name, mount);
        volumes.push(volume);
        blockers.extend(blocker);
    }
    volumes.sort_by(|left, right| left.target().cmp(right.target()));
    (volumes, blockers)
}

fn service_routes(service: &ServiceConfig) -> Vec<V7RouteInventory> {
    let domains = service
        .domain
        .iter()
        .chain(service.domains.iter().flatten())
        .chain(service.resolved_domain.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let scheme = service
        .scheme
        .as_deref()
        .unwrap_or(if service.localhost_tls {
            "https"
        } else {
            "http"
        });
    domains
        .into_iter()
        .map(|domain| V7RouteInventory::new(&service.name, domain, scheme, service.port))
        .collect()
}

fn volume_observation_blockers(
    service_id: &str,
    expected: &[V7VolumeInventory],
    observed: &[crate::control_plane::engine::ObservedContainerMount],
) -> Vec<V7InventoryBlocker> {
    let mut blockers = Vec::new();
    for volume in expected {
        let matching_target = observed
            .iter()
            .find(|mount| mount.target() == volume.target());
        if !matching_target.is_some_and(|mount| volume.matches_observed(mount)) {
            blockers.push(V7InventoryBlocker::VolumeMismatch {
                service_id: service_id.to_owned(),
                target: volume.target().to_owned(),
                expected_source: volume.expected_source().to_owned(),
                observed_source: matching_target.map(|mount| mount.source().to_owned()),
            });
        }
    }
    for mount in observed {
        if !expected
            .iter()
            .any(|volume| volume.target() == mount.target())
        {
            blockers.push(V7InventoryBlocker::UnexpectedVolume {
                service_id: service_id.to_owned(),
                source: mount.source().to_owned(),
                target: mount.target().to_owned(),
            });
        }
    }
    blockers
}

const fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Database => "database",
        Kind::Cache => "cache",
        Kind::ObjectStore => "object-store",
        Kind::Search => "search",
        Kind::App => "app",
    }
}

const fn default_data_target(driver: Driver) -> Option<&'static str> {
    match driver {
        Driver::Mongodb => Some("/data/db"),
        Driver::Postgres => Some("/var/lib/postgresql/data"),
        Driver::Mysql => Some("/var/lib/mysql"),
        Driver::Sqlserver => Some("/var/opt/mssql"),
        Driver::Redis | Driver::Valkey | Driver::Dragonfly => Some("/data"),
        Driver::Minio | Driver::Rustfs => Some("/data"),
        Driver::Garage => Some("/var/lib/garage"),
        Driver::Localstack => Some("/var/lib/localstack"),
        Driver::Opensearch => Some("/usr/share/opensearch/data"),
        Driver::Elasticsearch => Some("/usr/share/elasticsearch/data"),
        Driver::Meilisearch => Some("/meili_data"),
        Driver::Typesense => Some("/data"),
        Driver::Memcached
        | Driver::Frankenphp
        | Driver::Reverb
        | Driver::Horizon
        | Driver::Scheduler
        | Driver::Dusk
        | Driver::Gotenberg
        | Driver::Mailhog
        | Driver::Rabbitmq
        | Driver::Soketi => None,
    }
}
