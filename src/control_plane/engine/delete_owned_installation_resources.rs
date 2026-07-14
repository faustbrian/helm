use super::{
    ContainerDiscovery, ContainerLifecycle, ContainerState, EngineError, NetworkDiscovery,
    NetworkManager, ObservedResourceOwnership, OwnedContainer, OwnedNetwork, OwnedVolume,
    ResourceKind, RetentionClass, VolumeDiscovery, VolumeManager, reconstruct_owned_container,
    reconstruct_owned_network, reconstruct_owned_volume,
};

/// Deletes exact installation-owned Engine objects in dependency-safe order.
pub(crate) async fn delete_owned_installation_resources<E>(
    engine: &mut E,
    installation_id: &str,
    schema_version: u32,
) -> Result<(), EngineError>
where
    E: ContainerDiscovery
        + ContainerLifecycle
        + VolumeDiscovery
        + VolumeManager
        + NetworkDiscovery
        + NetworkManager,
{
    if installation_id.is_empty() || schema_version == 0 {
        return Err(EngineError::InvalidRequest {
            detail: "installation cleanup identity is incomplete".to_owned(),
        });
    }
    let mut containers = engine
        .discover_managed()
        .await?
        .iter()
        .filter_map(|observed| {
            owned_or_foreign(
                reconstruct_owned_container(observed, installation_id, schema_version),
                "container",
                observed.id().as_str(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut volumes = engine
        .discover_managed_volumes()
        .await?
        .iter()
        .filter_map(|observed| {
            owned_or_foreign(
                reconstruct_owned_volume(observed, installation_id, schema_version),
                "volume",
                observed.name(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut networks = engine
        .discover_managed_networks()
        .await?
        .iter()
        .filter_map(|observed| {
            owned_or_foreign(
                reconstruct_owned_network(observed, installation_id, schema_version),
                "network",
                observed.id().as_str(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    containers.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
    volumes.sort_by(|left, right| left.name().cmp(right.name()));
    networks.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
    validate_kinds(&containers, &volumes, &networks)?;

    for container in &containers {
        match engine.inspect(container).await? {
            ContainerState::Running => engine.stop(container).await?,
            ContainerState::Stopped => {}
            ContainerState::Missing => continue,
        }
        engine.remove(container).await?;
    }
    for volume in &volumes {
        engine.remove_volume(volume).await?;
    }
    for network in &networks {
        engine.remove_network(network).await?;
    }

    Ok(())
}

fn owned_or_foreign<T>(
    ownership: Result<T, ObservedResourceOwnership>,
    resource_kind: &str,
    resource_id: &str,
) -> Option<Result<T, EngineError>> {
    match ownership {
        Ok(owned) => Some(Ok(owned)),
        Err(
            ObservedResourceOwnership::Unmanaged
            | ObservedResourceOwnership::ForeignInstallation { .. },
        ) => None,
        Err(detail) => Some(Err(EngineError::InvalidRequest {
            detail: format!(
                "installation cleanup refused ambiguous {resource_kind} '{resource_id}': {detail:?}"
            ),
        })),
    }
}

fn validate_kinds(
    containers: &[OwnedContainer],
    volumes: &[OwnedVolume],
    networks: &[OwnedNetwork],
) -> Result<(), EngineError> {
    let invalid_container = containers.iter().find(|container| {
        matches!(
            container.metadata().kind(),
            ResourceKind::Volume | ResourceKind::Network
        )
    });
    let invalid_volume = volumes
        .iter()
        .find(|volume| volume.metadata().kind() != ResourceKind::Volume);
    let invalid_network = networks
        .iter()
        .find(|network| network.metadata().kind() != ResourceKind::Network);
    if invalid_container.is_some() || invalid_volume.is_some() || invalid_network.is_some() {
        return Err(EngineError::InvalidRequest {
            detail: "installation cleanup resource kind does not match its Engine object"
                .to_owned(),
        });
    }
    if let Some(volume) = volumes.iter().find(|volume| {
        volume.metadata().retention() == RetentionClass::Persistent
            && volume.metadata().project_id().is_some()
    }) {
        return Err(EngineError::InvalidRequest {
            detail: format!(
                "installation cleanup refuses project-owned persistent volume '{}' without explicit recovery authorization",
                volume.name()
            ),
        });
    }

    Ok(())
}
