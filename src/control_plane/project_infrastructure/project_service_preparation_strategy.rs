use super::{
    PreparedProjectService, ProjectServicePreparationError, opensearch_initial_admin_password,
    plan_elasticsearch_project_resources, plan_meilisearch_project_resources,
    plan_opensearch_project_resources, plan_soketi_project_resources,
    plan_typesense_project_resources,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::{ServiceDeploymentStrategy, ServiceExecutionPlan};

/// Selects the service-specific preparation adapter for one execution plan.
#[derive(Clone, Copy)]
pub(crate) enum ProjectServicePreparationStrategy {
    Elasticsearch,
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
            Some("elasticsearch" | "meilisearch" | "opensearch" | "soketi" | "typesense")
        ) || service.strategy() == ServiceDeploymentStrategy::DedicatedRoutableProject
    }

    pub(crate) fn finalize_candidate_secret(self, secret: CredentialSecret) -> CredentialSecret {
        match self {
            Self::OpenSearch => opensearch_initial_admin_password(secret),
            Self::Elasticsearch | Self::Meilisearch | Self::Soketi | Self::Typesense => secret,
        }
    }

    pub(crate) fn plan(
        self,
        service: &ServiceExecutionPlan,
        secret: CredentialSecret,
    ) -> Result<PreparedProjectService, ProjectServicePreparationError> {
        match self {
            Self::Elasticsearch => plan_elasticsearch_project_resources(service, secret),
            Self::Meilisearch => plan_meilisearch_project_resources(service, secret),
            Self::OpenSearch => plan_opensearch_project_resources(service, secret),
            Self::Soketi => plan_soketi_project_resources(service, secret),
            Self::Typesense => plan_typesense_project_resources(service, secret),
        }
    }
}
