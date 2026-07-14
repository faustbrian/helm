use super::bollard_engine_adapter::{
    backend_error, request_timeout, verify_v7_container_labels_for,
};
use super::{
    BollardEngineAdapter, EngineError, EngineFuture, V7ContainerRetirement,
    V7ContainerRetirementTarget, bounded_engine_operation,
};
use bollard::Docker;
use bollard::errors::Error as BollardError;
use bollard::models::MountPoint;
use bollard::query_parameters::{ListContainersOptionsBuilder, RemoveVolumeOptions};
use std::collections::HashMap;

impl V7ContainerRetirement for BollardEngineAdapter {
    fn retire_v7_container<'operation>(
        &'operation mut self,
        target: &'operation V7ContainerRetirementTarget,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("retire accepted v7 container", request_timeout(), async {
                let observed = match self
                    .docker
                    .inspect_container(target.container().container_id().as_str(), None)
                    .await
                {
                    Ok(observed) => observed,
                    Err(BollardError::DockerResponseServerError {
                        status_code: 404, ..
                    }) => {
                        let existing = existing_v7_retirement_volumes(&self.docker, target).await?;
                        return validate_missing_v7_container_retry(target, &existing);
                    }
                    Err(error) => {
                        return Err(backend_error("inspect accepted v7 container", error));
                    }
                };
                let labels = observed
                    .config
                    .and_then(|config| config.labels)
                    .unwrap_or_default();
                let mounts = observed.mounts.unwrap_or_default();
                verify_v7_container_retirement(target, &labels, &mounts)?;
                verify_v7_retirement_volumes_exist(&self.docker, target).await?;

                if observed.state.and_then(|state| state.running) == Some(true) {
                    stop_v7_container(&self.docker, target).await?;
                }
                remove_v7_container(&self.docker, target).await?;
                remove_v7_retirement_volumes(&self.docker, target).await
            })
            .await
        })
    }
}

async fn verify_v7_retirement_volumes_exist(
    docker: &Docker,
    target: &V7ContainerRetirementTarget,
) -> Result<(), EngineError> {
    for volume in target.named_volumes() {
        docker
            .inspect_volume(volume)
            .await
            .map_err(|error| backend_error("verify accepted v7 retirement volume", error))?;
    }

    Ok(())
}

async fn stop_v7_container(
    docker: &Docker,
    target: &V7ContainerRetirementTarget,
) -> Result<(), EngineError> {
    match docker
        .stop_container(target.container().container_id().as_str(), None)
        .await
    {
        Ok(())
        | Err(BollardError::DockerResponseServerError {
            status_code: 304, ..
        }) => Ok(()),
        Err(error) => Err(backend_error("stop accepted v7 container", error)),
    }
}

async fn remove_v7_container(
    docker: &Docker,
    target: &V7ContainerRetirementTarget,
) -> Result<(), EngineError> {
    match docker
        .remove_container(target.container().container_id().as_str(), None)
        .await
    {
        Ok(())
        | Err(BollardError::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(error) => Err(backend_error("remove accepted v7 container", error)),
    }
}

async fn remove_v7_retirement_volumes(
    docker: &Docker,
    target: &V7ContainerRetirementTarget,
) -> Result<(), EngineError> {
    for volume in target.named_volumes() {
        let users = docker
            .list_containers(Some(v7_volume_user_list_request(volume)))
            .await
            .map_err(|error| backend_error("verify accepted v7 volume is unused", error))?;
        if !users.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: format!(
                    "refusing to retire accepted v7 volume '{volume}' because an Engine container still uses it"
                ),
            });
        }
        match docker
            .remove_volume(volume, None::<RemoveVolumeOptions>)
            .await
        {
            Ok(())
            | Err(BollardError::DockerResponseServerError {
                status_code: 404, ..
            }) => {}
            Err(error) => return Err(backend_error("remove accepted v7 volume", error)),
        }
    }

    Ok(())
}

async fn existing_v7_retirement_volumes(
    docker: &Docker,
    target: &V7ContainerRetirementTarget,
) -> Result<Vec<String>, EngineError> {
    let mut existing = Vec::new();
    for volume in target.named_volumes() {
        match docker.inspect_volume(volume).await {
            Ok(_) => existing.push(volume.clone()),
            Err(BollardError::DockerResponseServerError {
                status_code: 404, ..
            }) => {}
            Err(error) => {
                return Err(backend_error(
                    "inspect accepted v7 retirement volume",
                    error,
                ));
            }
        }
    }

    Ok(existing)
}

pub(super) fn validate_missing_v7_container_retry(
    target: &V7ContainerRetirementTarget,
    existing_volumes: &[String],
) -> Result<(), EngineError> {
    if existing_volumes.is_empty() {
        return Ok(());
    }

    Err(EngineError::InvalidRequest {
        detail: format!(
            "accepted v7 container '{}' is already absent but named volumes [{}] remain; refusing ambiguous retry because Docker-compatible volumes have no immutable identity",
            target.container().container_id().as_str(),
            existing_volumes.join(", ")
        ),
    })
}

pub(super) fn verify_v7_container_retirement(
    target: &V7ContainerRetirementTarget,
    labels: &HashMap<String, String>,
    mounts: &[MountPoint],
) -> Result<(), EngineError> {
    verify_v7_container_labels_for("retire accepted v7", target.container(), labels)?;
    let mut named_volumes = mounts
        .iter()
        .filter(|mount| mount.typ.as_deref() == Some("volume"))
        .map(|mount| {
            mount
                .name
                .as_ref()
                .or(mount.source.as_ref())
                .cloned()
                .ok_or_else(|| EngineError::InvalidRequest {
                    detail: "accepted v7 container has an unnamed Engine volume mount".to_owned(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    named_volumes.sort();
    if named_volumes != target.named_volumes() {
        return Err(EngineError::InvalidRequest {
            detail: format!(
                "accepted v7 container '{}' named-volume mounts no longer match retirement evidence",
                target.container().container_id().as_str()
            ),
        });
    }

    Ok(())
}

pub(super) fn v7_volume_user_list_request(
    volume: &str,
) -> bollard::query_parameters::ListContainersOptions {
    ListContainersOptionsBuilder::default()
        .all(true)
        .filters(&HashMap::from([(
            "volume".to_owned(),
            vec![volume.to_owned()],
        )]))
        .build()
}
