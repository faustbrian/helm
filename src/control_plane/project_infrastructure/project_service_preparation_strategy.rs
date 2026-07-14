use super::{
    PreparedProjectService, ProjectServicePreparationError, opensearch_initial_admin_password,
    plan_elasticsearch_project_resources, plan_localstack_project_resources,
    plan_meilisearch_project_resources, plan_memcached_project_resources,
    plan_opensearch_project_resources, plan_soketi_project_resources,
    plan_typesense_project_resources,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::{ServiceDeploymentStrategy, ServiceExecutionPlan};

/// Selects the service-specific preparation adapter for one execution plan.
#[derive(Clone, Copy)]
pub(crate) enum ProjectServicePreparationStrategy {
    Elasticsearch,
    LocalStack,
    Memcached,
    Meilisearch,
    OpenSearch,
    Soketi,
    Typesense,
}

impl ProjectServicePreparationStrategy {
    pub(crate) fn resolve(
        service: &ServiceExecutionPlan,
    ) -> Result<Option<Self>, ProjectServicePreparationError> {
        let strategy = match service.desired().preset() {
            Some("elasticsearch") => Some(Self::Elasticsearch),
            Some("localstack") => Some(Self::LocalStack),
            Some("memcached") => Some(Self::Memcached),
            Some("meilisearch") => Some(Self::Meilisearch),
            Some("opensearch") => Some(Self::OpenSearch),
            Some("soketi") => Some(Self::Soketi),
            Some("typesense") => Some(Self::Typesense),
            _ => None,
        };
        if strategy.is_none()
            && service.strategy() == ServiceDeploymentStrategy::DedicatedRoutableProject
        {
            return Err(ProjectServicePreparationError::new(format!(
                "project service preset '{}' has no preparation strategy",
                service.desired().preset().unwrap_or("<none>")
            )));
        }

        Ok(strategy)
    }

    pub(crate) fn requires_preparation(service: &ServiceExecutionPlan) -> bool {
        matches!(
            service.desired().preset(),
            Some(
                "elasticsearch"
                    | "localstack"
                    | "meilisearch"
                    | "memcached"
                    | "opensearch"
                    | "soketi"
                    | "typesense"
            )
        ) || service.strategy() == ServiceDeploymentStrategy::DedicatedRoutableProject
    }

    pub(crate) const fn requires_credential(self) -> bool {
        !matches!(self, Self::LocalStack | Self::Memcached)
    }

    pub(crate) fn finalize_candidate_secret(self, secret: CredentialSecret) -> CredentialSecret {
        match self {
            Self::OpenSearch => opensearch_initial_admin_password(secret),
            Self::Elasticsearch
            | Self::LocalStack
            | Self::Meilisearch
            | Self::Memcached
            | Self::Soketi
            | Self::Typesense => secret,
        }
    }

    pub(crate) fn plan(
        self,
        service: &ServiceExecutionPlan,
        secret: Option<CredentialSecret>,
    ) -> Result<PreparedProjectService, ProjectServicePreparationError> {
        match self {
            Self::Elasticsearch => {
                plan_elasticsearch_project_resources(service, required(secret, "Elasticsearch")?)
            }
            Self::LocalStack => plan_without_credential(secret, "LocalStack", || {
                plan_localstack_project_resources(service)
            }),
            Self::Meilisearch => {
                plan_meilisearch_project_resources(service, required(secret, "Meilisearch")?)
            }
            Self::Memcached => plan_without_credential(secret, "Memcached", || {
                plan_memcached_project_resources(service)
            }),
            Self::OpenSearch => {
                plan_opensearch_project_resources(service, required(secret, "OpenSearch")?)
            }
            Self::Soketi => plan_soketi_project_resources(service, required(secret, "Soketi")?),
            Self::Typesense => {
                plan_typesense_project_resources(service, required(secret, "Typesense")?)
            }
        }
    }
}

fn plan_without_credential(
    secret: Option<CredentialSecret>,
    preset: &str,
    plan: impl FnOnce() -> Result<PreparedProjectService, ProjectServicePreparationError>,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if secret.is_some() {
        return Err(ProjectServicePreparationError::new(format!(
            "{preset} preparation does not accept credentials"
        )));
    }

    plan()
}

fn required(
    secret: Option<CredentialSecret>,
    preset: &str,
) -> Result<CredentialSecret, ProjectServicePreparationError> {
    secret.ok_or_else(|| {
        ProjectServicePreparationError::new(format!(
            "{preset} preparation requires generated credentials"
        ))
    })
}
