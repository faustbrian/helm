use super::{
    PreparedRedisSharedInstance, RedisAclSnapshot, RedisPreparationError, RedisPreparationOptions,
    RedisSharedInstancePlan, RedisSharedInstancePlanOptions, plan_redis_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
    shared_identity_hex,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

/// Reserves stable ACL secrets and composes one complete snapshot per instance.
pub(crate) fn prepare_redis_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared_instances: &[SharedInstancePlan],
    entropy: &Entropy,
    options: RedisPreparationOptions<'_>,
) -> Result<Vec<PreparedRedisSharedInstance>, RedisPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if !matches!(shared.profile().implementation(), "redis" | "valkey") {
            return Err(invalid(format!(
                "Redis-compatible preparation cannot materialize implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let identity = shared
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("Redis-compatible fingerprint is malformed"))?;
        let state_directory = options
            .state_directory
            .join("shared")
            .join(options.installation_id)
            .join(shared_identity_hex(identity))
            .join("redis-acl");
        let candidate = instance_plan(
            shared,
            &options,
            &state_directory,
            generate_credential_secret(entropy).map_err(invalid)?,
        )?;
        let bootstrap = store
            .insert_credential_if_absent(candidate.bootstrap_credential())
            .map_err(invalid)?;
        require_active(&bootstrap)?;
        let instance = instance_plan(
            shared,
            &options,
            &state_directory,
            CredentialSecret::new(bootstrap.secret().to_owned()),
        )?;
        let mut projects = Vec::with_capacity(shared.consumers().len());

        for consumer in shared.consumers() {
            let candidate = plan_redis_project_resources(
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
                plan_redis_project_resources(
                    consumer.project_id(),
                    consumer.service_id(),
                    &instance,
                    CredentialSecret::new(credential.secret().to_owned()),
                )
                .map_err(invalid)?,
            );
        }
        let snapshot = RedisAclSnapshot::new(
            CredentialSecret::new(bootstrap.secret().to_owned()),
            projects
                .iter()
                .map(|project| project.acl().clone())
                .collect(),
        )
        .map_err(invalid)?;
        prepared.push(PreparedRedisSharedInstance::new(
            instance,
            projects,
            snapshot,
            state_directory,
        ));
    }

    Ok(prepared)
}

fn instance_plan(
    shared: &SharedInstancePlan,
    options: &RedisPreparationOptions<'_>,
    state_directory: &std::path::Path,
    bootstrap_secret: CredentialSecret,
) -> Result<RedisSharedInstancePlan, RedisPreparationError> {
    RedisSharedInstancePlan::new(
        shared,
        RedisSharedInstancePlanOptions {
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: shared.fingerprint().as_str().to_owned(),
            acl_directory: state_directory.join("mounted"),
            bootstrap_secret,
        },
    )
    .map_err(invalid)
}

fn require_active(
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), RedisPreparationError> {
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "credential '{}' is disabled and requires explicit project adoption",
            credential.credential_id()
        )));
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> RedisPreparationError {
    RedisPreparationError::new(error.to_string())
}
