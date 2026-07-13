use futures_util::StreamExt;

use super::{ProjectLogMessage, ProjectLogRequest, ProjectLogTarget};
use crate::control_plane::daemon::ipc::IpcOutputStream;
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLogOptions, ContainerLogTail, EngineError, LogSource,
    ObservedResourceOwnership, OwnedContainer, reconstruct_owned_container,
};

/// Re-verifies live ownership and streams selected containers with backpressure.
pub(crate) async fn execute_project_logs<E>(
    engine: E,
    request: ProjectLogRequest,
    installation_id: String,
    schema_version: u32,
    sender: tokio::sync::mpsc::Sender<ProjectLogMessage>,
) -> Result<(), EngineError>
where
    E: Clone + ContainerDiscovery + LogSource + Send + Sync + 'static,
{
    let observed = engine.discover_managed().await?;
    let mut owned = Vec::new();
    for container in observed {
        match reconstruct_owned_container(&container, &installation_id, schema_version) {
            Ok(container) => owned.push(container),
            Err(ObservedResourceOwnership::Unmanaged)
            | Err(ObservedResourceOwnership::ForeignInstallation { .. }) => {}
            Err(ownership) => {
                return Err(EngineError::Backend {
                    detail: format!(
                        "managed container '{}' has invalid ownership while resolving project logs: {ownership:?}",
                        container.id().as_str()
                    ),
                });
            }
        }
    }
    let tail = match request.tail() {
        Some(lines) => ContainerLogTail::last(lines)?,
        None => ContainerLogTail::all(),
    };
    let options = ContainerLogOptions::new(request.follow(), tail);
    let mut tasks = tokio::task::JoinSet::new();

    for target in request.targets() {
        let container = exact_container(&owned, target)?;
        let engine = engine.clone();
        let sender = sender.clone();
        let service = target.service().to_owned();
        tasks.spawn(async move {
            let mut stream = engine.logs(&container, &options).await?;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                let stream = if chunk.is_stderr() {
                    IpcOutputStream::Stderr
                } else {
                    IpcOutputStream::Stdout
                };
                if sender
                    .send(ProjectLogMessage::new(
                        service.clone(),
                        stream,
                        chunk.bytes().to_vec(),
                    ))
                    .await
                    .is_err()
                {
                    return Ok(());
                }
            }

            Ok::<(), EngineError>(())
        });
    }
    drop(sender);

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tasks.abort_all();
                return Err(error);
            }
            Err(error) => {
                tasks.abort_all();
                return Err(EngineError::Backend {
                    detail: format!("project log task failed: {error}"),
                });
            }
        }
    }

    Ok(())
}

fn exact_container(
    containers: &[OwnedContainer],
    target: &ProjectLogTarget,
) -> Result<OwnedContainer, EngineError> {
    let matches = containers
        .iter()
        .filter(|container| {
            container.id().as_str() == target.resource_id()
                && container.metadata().project_id() == target.project_id()
        })
        .cloned()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [container] => Ok(container.clone()),
        [] => Err(EngineError::Backend {
            detail: format!(
                "project log service '{}' has no exact live owned container",
                target.service()
            ),
        }),
        _ => Err(EngineError::Backend {
            detail: format!(
                "project log service '{}' has duplicate live owned containers",
                target.service()
            ),
        }),
    }
}
