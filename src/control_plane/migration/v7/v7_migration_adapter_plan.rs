use super::{
    V7EnvironmentMigrationAdapter, V7MigrationServiceAdapter, V7RouteMigrationAdapter,
    V7TrustMigrationAdapter, V7VolumeMigrationAdapter,
};
use crate::control_plane::ServiceDeploymentStrategy;

/// One service's deterministic migration and volume ownership decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7MigrationServiceSelection {
    service_id: String,
    deployment_strategy: ServiceDeploymentStrategy,
    adapter: V7MigrationServiceAdapter,
    volume_adapter: V7VolumeMigrationAdapter,
    named_volumes: Vec<String>,
}

impl V7MigrationServiceSelection {
    pub(super) fn new(
        service_id: String,
        deployment_strategy: ServiceDeploymentStrategy,
        adapter: V7MigrationServiceAdapter,
        volume_adapter: V7VolumeMigrationAdapter,
        named_volumes: Vec<String>,
    ) -> Self {
        Self {
            service_id,
            deployment_strategy,
            adapter,
            volume_adapter,
            named_volumes,
        }
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) const fn adapter(&self) -> &V7MigrationServiceAdapter {
        &self.adapter
    }

    pub(crate) const fn deployment_strategy(&self) -> ServiceDeploymentStrategy {
        self.deployment_strategy
    }

    pub(crate) const fn volume_adapter(&self) -> V7VolumeMigrationAdapter {
        self.volume_adapter
    }

    pub(crate) fn named_volumes(&self) -> &[String] {
        &self.named_volumes
    }
}

/// Immutable, revision-bound adapter decisions for one accepted v7 inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7MigrationAdapterPlan {
    evidence_revision: String,
    plan_revision: String,
    services: Vec<V7MigrationServiceSelection>,
    route_adapter: V7RouteMigrationAdapter,
    trust_adapter: V7TrustMigrationAdapter,
    environment_adapter: V7EnvironmentMigrationAdapter,
}

impl V7MigrationAdapterPlan {
    pub(super) fn new(
        evidence_revision: String,
        plan_revision: String,
        services: Vec<V7MigrationServiceSelection>,
        route_adapter: V7RouteMigrationAdapter,
        trust_adapter: V7TrustMigrationAdapter,
        environment_adapter: V7EnvironmentMigrationAdapter,
    ) -> Self {
        Self {
            evidence_revision,
            plan_revision,
            services,
            route_adapter,
            trust_adapter,
            environment_adapter,
        }
    }

    pub(crate) fn evidence_revision(&self) -> &str {
        &self.evidence_revision
    }

    pub(crate) fn plan_revision(&self) -> &str {
        &self.plan_revision
    }

    pub(crate) fn services(&self) -> &[V7MigrationServiceSelection] {
        &self.services
    }

    pub(crate) const fn route_adapter(&self) -> V7RouteMigrationAdapter {
        self.route_adapter
    }

    pub(crate) const fn trust_adapter(&self) -> V7TrustMigrationAdapter {
        self.trust_adapter
    }

    pub(crate) const fn environment_adapter(&self) -> V7EnvironmentMigrationAdapter {
        self.environment_adapter
    }
}
