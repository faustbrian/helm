use super::{
    ObjectStorePreparationError, ObjectStorePreparationOptions, ObjectStoreSharedInstancePlan,
    ObjectStoreSharedInstancePlanOptions, PreparedObjectStoreSharedInstance,
    plan_object_store_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

/// Reserves stable MinIO root and project credentials before Engine mutation.
pub(crate) fn prepare_object_store_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared_instances: &[SharedInstancePlan],
    entropy: &Entropy,
    options: ObjectStorePreparationOptions<'_>,
) -> Result<Vec<PreparedObjectStoreSharedInstance>, ObjectStorePreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if shared.profile().implementation() != "minio" {
            return Err(invalid(format!(
                "object-store preparation cannot materialize unproven implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let identity = shared
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("object-store fingerprint is malformed"))?;
        let policy_directory = options
            .state_directory
            .join("shared")
            .join(identity)
            .join("object-store-policies");
        let candidate = instance_plan(
            shared,
            &options,
            &policy_directory,
            generate_credential_secret(entropy).map_err(invalid)?,
        )?;
        let root = store
            .insert_credential_if_absent(candidate.root_credential())
            .map_err(invalid)?;
        require_active(&root)?;
        let instance = instance_plan(
            shared,
            &options,
            &policy_directory,
            CredentialSecret::new(root.secret().to_owned()),
        )?;
        let mut projects = Vec::with_capacity(shared.consumers().len());

        for consumer in shared.consumers() {
            let candidate = plan_object_store_project_resources(
                consumer.project_id(),
                consumer.service_id(),
                &instance,
                generate_credential_secret(entropy).map_err(invalid)?,
            )
            .map_err(invalid)?;
            let credential = store
                .insert_credential_if_absent(candidate.credential())
                .map_err(invalid)?;
            require_active(&credential)?;
            projects.push(
                plan_object_store_project_resources(
                    consumer.project_id(),
                    consumer.service_id(),
                    &instance,
                    CredentialSecret::new(credential.secret().to_owned()),
                )
                .map_err(invalid)?,
            );
        }
        prepared.push(PreparedObjectStoreSharedInstance::new(
            instance,
            projects,
            policy_directory,
        ));
    }

    Ok(prepared)
}

fn instance_plan(
    shared: &SharedInstancePlan,
    options: &ObjectStorePreparationOptions<'_>,
    policy_directory: &std::path::Path,
    root_secret: CredentialSecret,
) -> Result<ObjectStoreSharedInstancePlan, ObjectStorePreparationError> {
    ObjectStoreSharedInstancePlan::new(
        shared,
        ObjectStoreSharedInstancePlanOptions {
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: shared.fingerprint().as_str().to_owned(),
            policy_directory: policy_directory.to_path_buf(),
            root_secret,
        },
    )
    .map_err(invalid)
}

fn require_active(
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), ObjectStorePreparationError> {
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "credential '{}' is disabled and requires explicit project adoption",
            credential.credential_id()
        )));
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> ObjectStorePreparationError {
    ObjectStorePreparationError::new(error.to_string())
}
