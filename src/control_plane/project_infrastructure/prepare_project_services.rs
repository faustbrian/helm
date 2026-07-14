use super::{
    PreparedProjectService, ProjectServicePreparationError, opensearch_initial_admin_password,
    plan_meilisearch_project_resources, plan_opensearch_project_resources,
    plan_soketi_project_resources, plan_typesense_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};
use crate::control_plane::{ExecutionPlan, ServiceDeploymentStrategy};

/// Reserves stable credentials for project services with generated state.
pub(crate) fn prepare_project_services<Store, Entropy>(
    store: &mut Store,
    execution: &ExecutionPlan,
    entropy: &Entropy,
) -> Result<Vec<PreparedProjectService>, ProjectServicePreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    execution
        .services()
        .iter()
        .filter(|service| {
            service.strategy() == ServiceDeploymentStrategy::DedicatedRoutableProject
                || matches!(
                    service.desired().preset(),
                    Some("meilisearch" | "opensearch" | "typesense")
                )
        })
        .map(|service| {
            let generated = generate_credential_secret(entropy).map_err(invalid)?;
            let generated = if service.desired().preset() == Some("opensearch") {
                opensearch_initial_admin_password(generated)
            } else {
                generated
            };
            let candidate = match service.desired().preset() {
                Some("meilisearch") => plan_meilisearch_project_resources(service, generated)?,
                Some("opensearch") => plan_opensearch_project_resources(service, generated)?,
                Some("soketi") => plan_soketi_project_resources(service, generated)?,
                Some("typesense") => plan_typesense_project_resources(service, generated)?,
                preset => {
                    return Err(invalid(format!(
                        "project service preset '{}' has no preparation strategy",
                        preset.unwrap_or("<none>")
                    )));
                }
            };
            let credential = store
                .insert_credential_if_absent(candidate.credential())
                .map_err(invalid)?;
            if credential.lifecycle() != CredentialLifecycle::Active {
                return Err(invalid(format!(
                    "credential '{}' is disabled and requires explicit project adoption",
                    credential.credential_id()
                )));
            }

            let stable = CredentialSecret::new(credential.secret().to_owned());
            match service.desired().preset() {
                Some("meilisearch") => plan_meilisearch_project_resources(service, stable),
                Some("opensearch") => plan_opensearch_project_resources(service, stable),
                Some("soketi") => plan_soketi_project_resources(service, stable),
                Some("typesense") => plan_typesense_project_resources(service, stable),
                _ => unreachable!("candidate preparation accepted only registered presets"),
            }
        })
        .collect()
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
