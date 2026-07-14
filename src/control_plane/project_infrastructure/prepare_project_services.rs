use super::{
    PreparedProjectService, ProjectServicePreparationError, ProjectServicePreparationStrategy,
};
use crate::control_plane::ExecutionPlan;
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

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
    let mut prepared = Vec::new();
    for service in execution.services() {
        let Some(strategy) = ProjectServicePreparationStrategy::resolve(service)? else {
            continue;
        };
        let candidate_secret = if strategy.requires_credential() {
            let generated = generate_credential_secret(entropy).map_err(invalid)?;
            Some(strategy.finalize_candidate_secret(generated))
        } else {
            None
        };
        let candidate = strategy.plan(service, candidate_secret)?;
        let stable = if let Some(candidate_credential) = candidate.credential() {
            let credential = store
                .insert_credential_if_absent(candidate_credential)
                .map_err(invalid)?;
            if credential.lifecycle() != CredentialLifecycle::Active {
                return Err(invalid(format!(
                    "credential '{}' is disabled and requires explicit project adoption",
                    credential.credential_id()
                )));
            }
            Some(CredentialSecret::new(credential.secret().to_owned()))
        } else {
            None
        };
        prepared.push(strategy.plan(service, stable)?);
    }

    Ok(prepared)
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
