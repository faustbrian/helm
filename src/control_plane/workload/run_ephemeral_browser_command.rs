use super::{EphemeralBrowserPlan, ProjectCommandPlan, run_project_command};
use crate::control_plane::engine::{
    AttachedCommandOutput, CommandExecutor, ContainerDiscovery, ContainerHealth,
    ContainerLifecycle, ContainerState, EngineError, HealthObserver, ObservedResourceOwnership,
    OwnedContainer, ResourceKind, reconstruct_owned_container,
};
use std::time::Duration;

const READINESS_TIMEOUT: Duration = Duration::from_secs(60);
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Runs one application command between browser readiness and guaranteed cleanup.
pub(crate) async fn run_ephemeral_browser_command<E>(
    engine: &mut E,
    application: &OwnedContainer,
    command: &ProjectCommandPlan,
    browser: &EphemeralBrowserPlan,
) -> Result<AttachedCommandOutput, EngineError>
where
    E: CommandExecutor + ContainerDiscovery + ContainerLifecycle + HealthObserver,
{
    validate(browser)?;
    remove_stale_session(engine, browser).await?;
    let container = engine.create(browser.request()).await?;
    if container.metadata() != browser.request().metadata() {
        let ownership = EngineError::Backend {
            detail: "Engine returned an ephemeral browser with unexpected ownership".to_owned(),
        };
        return match cleanup(engine, &container).await {
            Ok(()) => Err(ownership),
            Err(cleanup) => Err(EngineError::Backend {
                detail: format!("{ownership}; ephemeral browser cleanup also failed: {cleanup}"),
            }),
        };
    }

    let execution = async {
        engine.start(&container).await?;
        wait_until_ready(engine, &container).await?;
        let command = command.with_additional_environment(browser.command_environment())?;
        run_project_command(engine, application, &command).await
    }
    .await;
    let cleanup = cleanup(engine, &container).await;

    match (execution, cleanup) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(EngineError::Backend {
            detail: format!("{error}; ephemeral browser cleanup also failed: {cleanup}"),
        }),
    }
}

fn validate(browser: &EphemeralBrowserPlan) -> Result<(), EngineError> {
    let metadata = browser.request().metadata();
    if metadata.kind() != ResourceKind::EphemeralService
        || metadata.project_id().is_none()
        || metadata.resource_id().is_none()
        || browser.request().network().is_none()
        || browser.request().platform().is_none()
        || browser.request().health_check().is_none()
        || browser.request().restart_policy().is_some()
        || !browser.request().port_bindings().is_empty()
    {
        return Err(EngineError::InvalidRequest {
            detail: "ephemeral browser ownership and private runtime contract are incomplete"
                .to_owned(),
        });
    }

    Ok(())
}

async fn remove_stale_session<E>(
    engine: &mut E,
    browser: &EphemeralBrowserPlan,
) -> Result<(), EngineError>
where
    E: ContainerDiscovery + ContainerLifecycle,
{
    let expected = browser.request().metadata();
    let mut matches = Vec::new();
    for observed in engine.discover_managed().await? {
        match reconstruct_owned_container(
            &observed,
            expected.installation_id(),
            expected.schema_version(),
        ) {
            Ok(container)
                if container.metadata().kind() == ResourceKind::EphemeralService
                    && container.metadata().project_id() == expected.project_id()
                    && container.metadata().resource_id() == expected.resource_id() =>
            {
                matches.push(container);
            }
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => {}
            Err(ownership) => {
                return Err(EngineError::Backend {
                    detail: format!(
                        "managed container '{}' has invalid ownership while recovering an ephemeral browser: {ownership:?}",
                        observed.id().as_str()
                    ),
                });
            }
        }
    }

    match matches.as_slice() {
        [] => Ok(()),
        [container] => cleanup(engine, container).await,
        containers => Err(EngineError::Backend {
            detail: format!(
                "ephemeral browser '{}-{}' owns {} containers; refusing to guess",
                expected.project_id().unwrap_or_default(),
                expected.resource_id().unwrap_or_default(),
                containers.len()
            ),
        }),
    }
}

async fn wait_until_ready<E>(engine: &E, container: &OwnedContainer) -> Result<(), EngineError>
where
    E: HealthObserver,
{
    tokio::time::timeout(READINESS_TIMEOUT, async {
        loop {
            match engine.observe_health(container).await? {
                ContainerHealth::Healthy => return Ok(()),
                ContainerHealth::Missing | ContainerHealth::Stopped => {
                    return Err(EngineError::Backend {
                        detail: "ephemeral browser stopped before becoming ready".to_owned(),
                    });
                }
                ContainerHealth::RunningUnverified
                | ContainerHealth::Restarting
                | ContainerHealth::Starting
                | ContainerHealth::Unhealthy { .. } => {
                    tokio::time::sleep(READINESS_POLL_INTERVAL).await;
                }
            }
        }
    })
    .await
    .map_err(|_| EngineError::Timeout {
        action: "wait for ephemeral browser readiness".to_owned(),
        timeout_milliseconds: READINESS_TIMEOUT.as_millis() as u64,
    })?
}

async fn cleanup<E>(engine: &mut E, container: &OwnedContainer) -> Result<(), EngineError>
where
    E: ContainerLifecycle,
{
    match engine.inspect(container).await? {
        ContainerState::Running => engine.stop(container).await?,
        ContainerState::Stopped => {}
        ContainerState::Missing => return Ok(()),
    }

    engine.remove(container).await
}
