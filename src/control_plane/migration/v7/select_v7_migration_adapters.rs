use super::{
    V7EnvironmentMigrationAdapter, V7MigrationAdapterPlan, V7MigrationAdapterSelectionError,
    V7MigrationAdapterSelectionOptions, V7MigrationServiceAdapter, V7MigrationServiceSelection,
    V7RouteMigrationAdapter, V7TrustMigrationAdapter, V7VolumeMigrationAdapter,
};
use crate::control_plane::resolve_service_deployment_strategy;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Selects one explicit, immutable strategy for every accepted v7 resource.
pub(crate) fn select_v7_migration_adapters(
    options: V7MigrationAdapterSelectionOptions<'_>,
) -> Result<V7MigrationAdapterPlan, V7MigrationAdapterSelectionError> {
    validate_evidence_revision(options.evidence_revision)?;
    let mut service_ids = BTreeSet::new();
    let mut deployment_strategies = BTreeMap::new();
    let mut services = Vec::with_capacity(options.services.len());
    for source in options.services {
        if source.service_id.is_empty() || !service_ids.insert(source.service_id) {
            return Err(V7MigrationAdapterSelectionError::new(format!(
                "v7 migration service id '{}' is empty or duplicated",
                source.service_id
            )));
        }
        let adapter = service_adapter(source.driver).ok_or_else(|| {
            V7MigrationAdapterSelectionError::new(format!(
                "v7 service '{}' driver '{}' has no migration adapter",
                source.service_id, source.driver
            ))
        })?;
        let volume_adapter = volume_adapter(adapter, !source.named_volumes.is_empty());
        let deployment_strategy =
            resolve_service_deployment_strategy(source.driver).map_err(|_| {
                V7MigrationAdapterSelectionError::new(format!(
                    "v7 service '{}' driver '{}' has no deployment strategy",
                    source.service_id, source.driver
                ))
            })?;
        deployment_strategies.insert(source.service_id, deployment_strategy);
        let mut named_volumes = source
            .named_volumes
            .iter()
            .map(|volume| (*volume).to_owned())
            .collect::<Vec<_>>();
        named_volumes.sort();
        if named_volumes.iter().any(String::is_empty)
            || named_volumes.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(V7MigrationAdapterSelectionError::new(format!(
                "v7 service '{}' has an empty or duplicated named volume",
                source.service_id
            )));
        }
        if volume_adapter != V7VolumeMigrationAdapter::NamedVolumeArchive {
            named_volumes.clear();
        }
        services.push(V7MigrationServiceSelection::new(
            source.service_id.to_owned(),
            deployment_strategy,
            adapter,
            volume_adapter,
            named_volumes,
        ));
    }
    services.sort_by(|left, right| left.service_id().cmp(right.service_id()));

    validate_routes(options.routes, &deployment_strategies)?;
    let route_adapter = if options.routes.is_empty() {
        V7RouteMigrationAdapter::NoRoutes
    } else {
        V7RouteMigrationAdapter::GatewaySnapshotCutover
    };
    let trust_adapter = trust_adapter(&options)?;
    let environment_adapter = environment_adapter(&options)?;
    let plan_revision = plan_revision(
        options.evidence_revision,
        &services,
        options.routes,
        route_adapter,
        trust_adapter,
        environment_adapter,
    );

    Ok(V7MigrationAdapterPlan::new(
        options.evidence_revision.to_owned(),
        plan_revision,
        services,
        route_adapter,
        trust_adapter,
        environment_adapter,
    ))
}

fn service_adapter(driver: &str) -> Option<V7MigrationServiceAdapter> {
    use V7MigrationServiceAdapter as Adapter;

    Some(match driver {
        "mongodb" => Adapter::MongoDbLogicalDatabase,
        "postgres" => Adapter::PostgresLogicalDatabase,
        "mysql" => Adapter::MySqlLogicalDatabase,
        "sqlserver" => Adapter::SqlServerLogicalDatabase,
        "redis" => Adapter::RedisTenantPrefix,
        "valkey" => Adapter::ValkeyTenantPrefix,
        "minio" => Adapter::MinioBucket,
        "rabbitmq" => Adapter::RabbitMqVhost,
        "dusk" => Adapter::RecreateEphemeral,
        "frankenphp" | "reverb" | "horizon" | "scheduler" => Adapter::RecreateProjectWorkload,
        "memcached" | "gotenberg" | "mailhog" | "soketi" => Adapter::RecreateStateless,
        "dragonfly" | "garage" | "rustfs" | "localstack" | "opensearch" | "elasticsearch"
        | "meilisearch" | "typesense" => Adapter::RecreateStateless,
        _ => return None,
    })
}

const fn volume_adapter(
    service_adapter: V7MigrationServiceAdapter,
    has_named_volumes: bool,
) -> V7VolumeMigrationAdapter {
    if !has_named_volumes {
        return V7VolumeMigrationAdapter::NoNamedVolumes;
    }
    if matches!(
        service_adapter,
        V7MigrationServiceAdapter::MongoDbLogicalDatabase
            | V7MigrationServiceAdapter::PostgresLogicalDatabase
            | V7MigrationServiceAdapter::MySqlLogicalDatabase
            | V7MigrationServiceAdapter::SqlServerLogicalDatabase
            | V7MigrationServiceAdapter::RedisTenantPrefix
            | V7MigrationServiceAdapter::ValkeyTenantPrefix
            | V7MigrationServiceAdapter::MinioBucket
            | V7MigrationServiceAdapter::RabbitMqVhost
    ) {
        V7VolumeMigrationAdapter::LogicalDataOwnsStorage
    } else {
        V7VolumeMigrationAdapter::NamedVolumeArchive
    }
}

fn validate_routes(
    routes: &[super::V7MigrationRouteSource<'_>],
    deployment_strategies: &BTreeMap<&str, crate::control_plane::ServiceDeploymentStrategy>,
) -> Result<(), V7MigrationAdapterSelectionError> {
    let mut domains = BTreeSet::new();
    for route in routes {
        let Some(strategy) = deployment_strategies.get(route.service_id) else {
            return Err(V7MigrationAdapterSelectionError::new(format!(
                "v7 route '{}' references unknown service '{}'",
                route.domain, route.service_id
            )));
        };
        if !strategy.claims_gateway_route() {
            return Err(V7MigrationAdapterSelectionError::new(format!(
                "v7 route '{}' service '{}' cannot claim a v8 gateway route with deployment strategy '{}'",
                route.domain,
                route.service_id,
                strategy.label()
            )));
        }
        if route.domain.is_empty() || !domains.insert(route.domain) {
            return Err(V7MigrationAdapterSelectionError::new(format!(
                "v7 route domain '{}' is empty or duplicated",
                route.domain
            )));
        }
        if !matches!(route.scheme, "http" | "https") || route.host_port == 0 {
            return Err(V7MigrationAdapterSelectionError::new(format!(
                "v7 route '{}' has unsupported origin '{}:{}'",
                route.domain, route.scheme, route.host_port
            )));
        }
    }
    Ok(())
}

fn trust_adapter(
    options: &V7MigrationAdapterSelectionOptions<'_>,
) -> Result<V7TrustMigrationAdapter, V7MigrationAdapterSelectionError> {
    if options.requires_legacy_ca_capture && options.captured_ca_certificates == 0 {
        return Err(V7MigrationAdapterSelectionError::new(
            "v7 migration requires legacy Caddy CA capture but accepted evidence contains no certificate",
        ));
    }
    Ok(if options.captured_ca_certificates == 0 {
        V7TrustMigrationAdapter::NoLegacyTrustTransition
    } else {
        V7TrustMigrationAdapter::InstallationLegacyCaddyCaTransition
    })
}

fn environment_adapter(
    options: &V7MigrationAdapterSelectionOptions<'_>,
) -> Result<V7EnvironmentMigrationAdapter, V7MigrationAdapterSelectionError> {
    if options.generated_environment_present && !options.protected_generated_environment {
        return Err(V7MigrationAdapterSelectionError::new(
            "v7 migration requires protected generated-environment rollback before adapter selection",
        ));
    }
    Ok(if options.generated_environment_present {
        V7EnvironmentMigrationAdapter::ProtectedGeneratedEnvironment
    } else {
        V7EnvironmentMigrationAdapter::NoGeneratedEnvironment
    })
}

fn validate_evidence_revision(revision: &str) -> Result<(), V7MigrationAdapterSelectionError> {
    if revision.len() == 64
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(V7MigrationAdapterSelectionError::new(
            "v7 migration adapter selection requires a lowercase SHA-256 evidence revision",
        ))
    }
}

fn plan_revision(
    evidence_revision: &str,
    services: &[V7MigrationServiceSelection],
    routes: &[super::V7MigrationRouteSource<'_>],
    route_adapter: V7RouteMigrationAdapter,
    trust_adapter: V7TrustMigrationAdapter,
    environment_adapter: V7EnvironmentMigrationAdapter,
) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "v1");
    hash_field(&mut hasher, evidence_revision);
    hash_field(&mut hasher, route_adapter.label());
    hash_field(&mut hasher, trust_adapter.label());
    hash_field(&mut hasher, environment_adapter.label());
    hasher.update((services.len() as u64).to_be_bytes());
    for service in services {
        hash_field(&mut hasher, service.service_id());
        hash_field(&mut hasher, service.deployment_strategy().label());
        hash_field(&mut hasher, service.adapter().label());
        hash_field(&mut hasher, service.volume_adapter().label());
        hasher.update((service.named_volumes().len() as u64).to_be_bytes());
        for volume in service.named_volumes() {
            hash_field(&mut hasher, volume);
        }
    }
    let mut routes = routes.to_vec();
    routes.sort_by(|left, right| {
        (left.domain, left.service_id, left.scheme, left.host_port).cmp(&(
            right.domain,
            right.service_id,
            right.scheme,
            right.host_port,
        ))
    });
    hasher.update((routes.len() as u64).to_be_bytes());
    for route in routes {
        hash_field(&mut hasher, route.domain);
        hash_field(&mut hasher, route.service_id);
        hash_field(&mut hasher, route.scheme);
        hasher.update(route.host_port.to_be_bytes());
    }
    hex::encode(hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}
