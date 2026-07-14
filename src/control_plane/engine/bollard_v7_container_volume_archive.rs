use super::bollard_engine_adapter::{
    backend_error, request_timeout, verify_v7_container_labels_for,
};
use super::{
    BollardEngineAdapter, ContainerState, EngineError, EngineFuture, V7ContainerCommandTarget,
    V7ContainerVolumeArchive, VolumeMount, bounded_engine_operation,
};
use bollard::errors::Error as BollardError;
use bollard::models::MountPoint;
use bollard::query_parameters::DownloadFromContainerOptionsBuilder;
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::io::{AsyncWrite, AsyncWriteExt};

impl V7ContainerVolumeArchive for BollardEngineAdapter {
    fn inspect_v7_volume_container<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        mounts: &'operation [VolumeMount],
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async move {
            bounded_engine_operation(
                "inspect accepted v7 volume container",
                request_timeout(),
                async {
                    match inspect_exact(&self.docker, target, mounts).await? {
                        Some(observed) => Ok(
                            if observed.state.and_then(|state| state.running) == Some(true) {
                                ContainerState::Running
                            } else {
                                ContainerState::Stopped
                            },
                        ),
                        None => Ok(ContainerState::Missing),
                    }
                },
            )
            .await
        })
    }

    fn start_v7_volume_container<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        mounts: &'operation [VolumeMount],
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation(
                "start accepted v7 volume container",
                request_timeout(),
                async {
                    require_exact(&self.docker, target, mounts).await?;
                    match self
                        .docker
                        .start_container(target.container_id().as_str(), None)
                        .await
                    {
                        Ok(())
                        | Err(BollardError::DockerResponseServerError {
                            status_code: 304, ..
                        }) => Ok(()),
                        Err(error) => {
                            Err(backend_error("start accepted v7 volume container", error))
                        }
                    }
                },
            )
            .await
        })
    }

    fn stop_v7_volume_container<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        mounts: &'operation [VolumeMount],
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation(
                "stop accepted v7 volume container",
                request_timeout(),
                async {
                    require_exact(&self.docker, target, mounts).await?;
                    match self
                        .docker
                        .stop_container(target.container_id().as_str(), None)
                        .await
                    {
                        Ok(())
                        | Err(BollardError::DockerResponseServerError {
                            status_code: 304, ..
                        }) => Ok(()),
                        Err(error) => {
                            Err(backend_error("stop accepted v7 volume container", error))
                        }
                    }
                },
            )
            .await
        })
    }

    fn download_v7_volume_archive<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        mounts: &'operation [VolumeMount],
        volume_name: &'operation str,
        output: &'operation mut (dyn AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            let mount = exact_requested_mount(mounts, volume_name)?;
            bounded_engine_operation(
                "download accepted v7 volume archive",
                request_timeout(),
                async {
                    require_exact(&self.docker, target, mounts).await?;
                    let options = DownloadFromContainerOptionsBuilder::default()
                        .path(mount.target())
                        .build();
                    let mut archive = self
                        .docker
                        .download_from_container(target.container_id().as_str(), Some(options));
                    while let Some(chunk) = archive.next().await {
                        output
                            .write_all(&chunk.map_err(|error| {
                                backend_error("download accepted v7 volume archive", error)
                            })?)
                            .await
                            .map_err(|error| EngineError::Backend {
                                detail: format!("write accepted v7 volume archive: {error}"),
                            })?;
                    }
                    output.flush().await.map_err(|error| EngineError::Backend {
                        detail: format!("flush accepted v7 volume archive: {error}"),
                    })
                },
            )
            .await
        })
    }
}

async fn inspect_exact(
    docker: &bollard::Docker,
    target: &V7ContainerCommandTarget,
    mounts: &[VolumeMount],
) -> Result<Option<bollard::models::ContainerInspectResponse>, EngineError> {
    let observed = match docker
        .inspect_container(target.container_id().as_str(), None)
        .await
    {
        Ok(observed) => observed,
        Err(error) => match error {
            BollardError::DockerResponseServerError {
                status_code: 404, ..
            } => return Ok(None),
            error => return Err(backend_error("inspect accepted v7 volume container", error)),
        },
    };
    verify_v7_volume_archive_target(
        target,
        mounts,
        &observed
            .config
            .as_ref()
            .and_then(|config| config.labels.as_ref())
            .cloned()
            .unwrap_or_default(),
        observed.mounts.as_deref().unwrap_or_default(),
    )?;

    Ok(Some(observed))
}

async fn require_exact(
    docker: &bollard::Docker,
    target: &V7ContainerCommandTarget,
    mounts: &[VolumeMount],
) -> Result<bollard::models::ContainerInspectResponse, EngineError> {
    inspect_exact(docker, target, mounts)
        .await?
        .ok_or_else(|| EngineError::InvalidRequest {
            detail: format!(
                "accepted v7 volume container '{}' is missing",
                target.container_id().as_str()
            ),
        })
}

fn exact_requested_mount<'mount>(
    mounts: &'mount [VolumeMount],
    volume_name: &str,
) -> Result<&'mount VolumeMount, EngineError> {
    let matching = mounts
        .iter()
        .filter(|mount| mount.source() == volume_name)
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [mount] => Ok(*mount),
        _ => Err(EngineError::InvalidRequest {
            detail: format!("accepted v7 archive volume '{volume_name}' is missing or ambiguous"),
        }),
    }
}

pub(super) fn verify_v7_volume_archive_target(
    target: &V7ContainerCommandTarget,
    expected_mounts: &[VolumeMount],
    labels: &HashMap<String, String>,
    observed_mounts: &[MountPoint],
) -> Result<(), EngineError> {
    verify_v7_container_labels_for("archive accepted v7 volume", target, labels)?;
    let mut expected = expected_mounts
        .iter()
        .map(|mount| {
            (
                mount.source().to_owned(),
                mount.target().to_owned(),
                !mount.is_read_only(),
            )
        })
        .collect::<Vec<_>>();
    expected.sort();
    if expected.is_empty() || expected.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(EngineError::InvalidRequest {
            detail: "accepted v7 volume archive evidence is empty or ambiguous".to_owned(),
        });
    }
    let mut observed = observed_mounts
        .iter()
        .filter(|mount| mount.typ.as_deref() == Some("volume"))
        .map(|mount| {
            Ok((
                mount
                    .name
                    .as_ref()
                    .or(mount.source.as_ref())
                    .cloned()
                    .ok_or_else(|| invalid_observed_mount(target))?,
                mount
                    .destination
                    .clone()
                    .ok_or_else(|| invalid_observed_mount(target))?,
                mount.rw.ok_or_else(|| invalid_observed_mount(target))?,
            ))
        })
        .collect::<Result<Vec<_>, EngineError>>()?;
    observed.sort();
    if expected != observed {
        return Err(invalid_observed_mount(target));
    }

    Ok(())
}

fn invalid_observed_mount(target: &V7ContainerCommandTarget) -> EngineError {
    EngineError::InvalidRequest {
        detail: format!(
            "accepted v7 container '{}' named-volume mounts no longer match archive evidence",
            target.container_id().as_str()
        ),
    }
}
