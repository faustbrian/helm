use super::{
    PreparedProjectService, ProjectServicePreparationError, plan_soketi_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};
use crate::control_plane::{ExecutionPlan, ServiceDeploymentStrategy};

/// Reserves stable credentials for every dedicated routable project service.
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
        .filter(|service| service.strategy() == ServiceDeploymentStrategy::DedicatedRoutableProject)
        .map(|service| {
            let candidate = plan_soketi_project_resources(
                service,
                generate_credential_secret(entropy).map_err(invalid)?,
            )?;
            let credential = store
                .insert_credential_if_absent(candidate.credential())
                .map_err(invalid)?;
            if credential.lifecycle() != CredentialLifecycle::Active {
                return Err(invalid(format!(
                    "credential '{}' is disabled and requires explicit project adoption",
                    credential.credential_id()
                )));
            }

            plan_soketi_project_resources(
                service,
                CredentialSecret::new(credential.secret().to_owned()),
            )
        })
        .collect()
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
