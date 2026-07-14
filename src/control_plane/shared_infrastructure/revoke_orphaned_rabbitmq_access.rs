use super::{
    CredentialSecret, OrphanedRabbitMqAccessOptions, RabbitMqProjectDefinition,
    SharedInfrastructureReconcileError, revoke_rabbitmq_project_access,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, ContainerState, EngineError,
    ManagedResourceMetadata, ObservedResourceOwnership, OwnedContainer, ResourceKind,
    RetentionClass, reconstruct_owned_container,
};
use crate::control_plane::state::{
    CredentialLifecycle, LogicalResourceRecord, ResourceLifecycle, ResourceRecord,
    ResourceRetention,
};

/// Revokes disabled RabbitMQ users while retaining their vhosts and messages.
pub(crate) async fn revoke_orphaned_rabbitmq_access<E>(
    engine: &mut E,
    options: OrphanedRabbitMqAccessOptions<'_>,
) -> Result<usize, SharedInfrastructureReconcileError>
where
    E: CommandExecutor + ContainerDiscovery + ContainerLifecycle,
{
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("discover RabbitMQ access targets", error))?;
    let mut shared = Vec::new();
    for container in &observed {
        match reconstruct_owned_container(
            container,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(owned) if owned.metadata().kind() == ResourceKind::SharedService => {
                shared.push(owned);
            }
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => {}
            Err(ownership) => {
                return Err(conflict(format!(
                    "managed container '{}' has invalid ownership while revoking RabbitMQ access: {ownership:?}",
                    container.id().as_str()
                )));
            }
        }
    }

    let mut revoked = 0;
    for logical in options.logical_resources.iter().filter(|logical| {
        logical.kind() == "rabbitmq_vhost_user"
            && logical.lifecycle() == ResourceLifecycle::Orphaned
    }) {
        let credential = options
            .credentials
            .iter()
            .find(|credential| credential.credential_id() == logical.logical_resource_id())
            .ok_or_else(|| {
                conflict(format!(
                    "orphaned RabbitMQ tenant '{}' has no retained credential",
                    logical.logical_resource_id()
                ))
            })?;
        validate_credential(logical, credential)?;
        let definition = RabbitMqProjectDefinition::new(
            logical.project_id(),
            logical.service_id(),
            CredentialSecret::new(credential.secret().to_owned()),
        )
        .map_err(|error| conflict(error.to_string()))?;
        if definition.username() != credential.username() {
            return Err(conflict(format!(
                "orphaned RabbitMQ tenant '{}' credential identity does not match its deterministic project user",
                logical.logical_resource_id()
            )));
        }

        let candidates = shared
            .iter()
            .filter(|container| {
                container.metadata().compatibility_fingerprint()
                    == logical.compatibility_fingerprint()
            })
            .collect::<Vec<_>>();
        let Some(container) = exact_candidate(logical, &candidates)? else {
            continue;
        };
        let resource = options
            .resources
            .iter()
            .find(|resource| resource.resource_id() == container.id().as_str())
            .ok_or_else(|| {
                conflict(format!(
                    "RabbitMQ shared service '{}' has no durable ownership record",
                    container.id().as_str()
                ))
            })?;
        if !matches_durable_ownership(resource, container.metadata()) {
            return Err(conflict(format!(
                "RabbitMQ shared service '{}' differs from its durable ownership record",
                container.id().as_str()
            )));
        }

        match engine
            .inspect(container)
            .await
            .map_err(|error| engine_error("inspect RabbitMQ access target", error))?
        {
            ContainerState::Running => {}
            ContainerState::Stopped => engine
                .start(container)
                .await
                .map_err(|error| engine_error("start RabbitMQ access target", error))?,
            ContainerState::Missing => continue,
        }
        if revoke_rabbitmq_project_access(engine, container, &definition)
            .await
            .map_err(|error| engine_error("revoke orphaned RabbitMQ access", error))?
        {
            revoked += 1;
        }
    }

    Ok(revoked)
}

fn validate_credential(
    logical: &LogicalResourceRecord,
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), SharedInfrastructureReconcileError> {
    if credential.lifecycle() != CredentialLifecycle::Disabled
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
    {
        return Err(conflict(format!(
            "orphaned RabbitMQ tenant '{}' requires its exact disabled project credential",
            logical.logical_resource_id()
        )));
    }

    Ok(())
}

fn exact_candidate<'container>(
    logical: &LogicalResourceRecord,
    candidates: &[&'container OwnedContainer],
) -> Result<Option<&'container OwnedContainer>, SharedInfrastructureReconcileError> {
    match candidates {
        [] => Ok(None),
        [container] => Ok(Some(*container)),
        _ => Err(conflict(format!(
            "orphaned RabbitMQ tenant '{}' matches multiple shared services",
            logical.logical_resource_id()
        ))),
    }
}

fn matches_durable_ownership(
    resource: &ResourceRecord,
    metadata: &ManagedResourceMetadata,
) -> bool {
    resource.installation_id() == metadata.installation_id()
        && resource.kind() == metadata.kind().label()
        && resource.scope_id() == metadata.resource_id()
        && resource.compatibility_fingerprint() == metadata.compatibility_fingerprint()
        && resource.project_id() == metadata.project_id()
        && resource.schema_version() == metadata.schema_version()
        && resource.desired_revision() == metadata.desired_revision()
        && resource.retention() == retention(metadata.retention())
        && resource.lifecycle() == ResourceLifecycle::Active
}

const fn retention(retention: RetentionClass) -> ResourceRetention {
    match retention {
        RetentionClass::Persistent => ResourceRetention::Persistent,
        RetentionClass::Disposable => ResourceRetention::Disposable,
        RetentionClass::BuildCache => ResourceRetention::BuildCache,
    }
}

fn conflict(detail: String) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Conflict { detail }
}

fn engine_error(action: &str, error: EngineError) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
