use super::{ProjectCommandExecutionOptions, ProjectCommandExecutionResult};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver,
    ObservedResourceOwnership, OwnedContainer, ResourceKind, reconstruct_owned_container,
};
use crate::control_plane::workload::{run_ephemeral_browser_command, run_project_command};

/// Resolves exact live ownership, then executes only in the requested app.
pub(crate) async fn execute_queued_project_command<E>(
    mut engine: E,
    options: ProjectCommandExecutionOptions,
) -> ProjectCommandExecutionResult
where
    E: ContainerDiscovery + CommandExecutor + ContainerLifecycle + HealthObserver,
{
    let operation = options.operation;
    let operation_id = operation.operation_id().to_owned();
    let outcome = async {
        let browser = options.ephemeral_browser.transpose()?;
        let managed_environment = options.managed_environment?;
        let plan = operation
            .plan()
            .with_additional_environment(&managed_environment)?;
        let application = find_application(
            &engine,
            operation.plan().project_id(),
            operation.service_id(),
            &options.installation_id,
            options.schema_version,
        )
        .await?;

        match browser {
            Some(browser) => {
                run_ephemeral_browser_command(&mut engine, &application, &plan, &browser).await
            }
            None => run_project_command(&engine, &application, &plan).await,
        }
    }
    .await;

    ProjectCommandExecutionResult::new(operation_id, outcome)
}

async fn find_application<E>(
    engine: &E,
    project_id: &str,
    service_id: &str,
    installation_id: &str,
    schema_version: u32,
) -> Result<OwnedContainer, crate::control_plane::engine::EngineError>
where
    E: ContainerDiscovery,
{
    let mut matches = Vec::new();
    for observed in engine.discover_managed().await? {
        match reconstruct_owned_container(&observed, installation_id, schema_version) {
            Ok(container)
                if container.metadata().kind() == ResourceKind::ProjectApplication
                    && container.metadata().project_id() == Some(project_id)
                    && container.metadata().resource_id() == Some(service_id) =>
            {
                matches.push(container);
            }
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => {}
            Err(ownership) => {
                return Err(crate::control_plane::engine::EngineError::Backend {
                    detail: format!(
                        "managed container '{}' has invalid ownership while resolving project command: {ownership:?}",
                        observed.id().as_str()
                    ),
                });
            }
        }
    }

    match matches.as_slice() {
        [application] => Ok(application.clone()),
        [] => Err(crate::control_plane::engine::EngineError::Backend {
            detail: format!(
                "project application '{project_id}-{service_id}' is not ready for command execution"
            ),
        }),
        _ => Err(crate::control_plane::engine::EngineError::Backend {
            detail: format!(
                "project application '{project_id}-{service_id}' has duplicate owned containers"
            ),
        }),
    }
}
