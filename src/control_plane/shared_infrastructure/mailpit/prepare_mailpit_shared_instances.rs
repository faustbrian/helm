use super::{
    MailpitAuthenticationSnapshot, MailpitPreparationError, MailpitPreparationOptions,
    MailpitProjectResources, MailpitSharedInstancePlan, MailpitSharedInstancePlanOptions,
    PreparedMailpitSharedInstance, plan_mailpit_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

const PROVISIONAL_AUTHENTICATION_REVISION: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// Reserves stable SMTP secrets and composes one complete authentication file.
pub(crate) fn prepare_mailpit_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared_instances: &[SharedInstancePlan],
    entropy: &Entropy,
    options: MailpitPreparationOptions<'_>,
) -> Result<Vec<PreparedMailpitSharedInstance>, MailpitPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if shared.profile().implementation() != "mailpit" {
            return Err(invalid(format!(
                "Mailpit preparation cannot materialize implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let identity = shared
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("Mailpit fingerprint is malformed"))?;
        let state_directory = options
            .state_directory
            .join("shared")
            .join(identity)
            .join("mailpit-authentication");
        let provisional = instance_plan(
            shared,
            &options,
            &state_directory,
            PROVISIONAL_AUTHENTICATION_REVISION,
        )?;
        let projects = stable_projects(store, entropy, shared, &provisional)?;
        let snapshot = MailpitAuthenticationSnapshot::new(
            projects
                .iter()
                .map(|project| project.definition().clone())
                .collect(),
        )
        .map_err(invalid)?;
        let instance = instance_plan(shared, &options, &state_directory, snapshot.revision())?;
        let projects = projects
            .iter()
            .map(|project| {
                plan_mailpit_project_resources(
                    project
                        .credential()
                        .project_id()
                        .expect("project Mailpit credential owner"),
                    project.credential().service_id(),
                    &instance,
                    CredentialSecret::new(project.credential().secret().to_owned()),
                )
                .map_err(invalid)
            })
            .collect::<Result<Vec<_>, _>>()?;
        prepared.push(PreparedMailpitSharedInstance::new(
            instance,
            projects,
            snapshot,
            state_directory,
        ));
    }

    Ok(prepared)
}

fn stable_projects<Store, Entropy>(
    store: &mut Store,
    entropy: &Entropy,
    shared: &SharedInstancePlan,
    instance: &MailpitSharedInstancePlan,
) -> Result<Vec<MailpitProjectResources>, MailpitPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut projects = Vec::with_capacity(shared.consumers().len());

    for consumer in shared.consumers() {
        let candidate = plan_mailpit_project_resources(
            consumer.project_id(),
            consumer.service_id(),
            instance,
            generate_credential_secret(entropy).map_err(invalid)?,
        )
        .map_err(invalid)?;
        let credential = store
            .insert_credential_if_absent(candidate.credential())
            .map_err(invalid)?;
        require_active(&credential)?;
        projects.push(
            plan_mailpit_project_resources(
                consumer.project_id(),
                consumer.service_id(),
                instance,
                CredentialSecret::new(credential.secret().to_owned()),
            )
            .map_err(invalid)?,
        );
    }

    Ok(projects)
}

fn instance_plan(
    shared: &SharedInstancePlan,
    options: &MailpitPreparationOptions<'_>,
    state_directory: &std::path::Path,
    authentication_revision: &str,
) -> Result<MailpitSharedInstancePlan, MailpitPreparationError> {
    MailpitSharedInstancePlan::new(
        shared,
        MailpitSharedInstancePlanOptions {
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: shared.fingerprint().as_str().to_owned(),
            authentication_directory: state_directory.join("mounted"),
            authentication_revision: authentication_revision.to_owned(),
        },
    )
    .map_err(invalid)
}

fn require_active(
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), MailpitPreparationError> {
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "credential '{}' is disabled and requires explicit project adoption",
            credential.credential_id()
        )));
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> MailpitPreparationError {
    MailpitPreparationError::new(error.to_string())
}
