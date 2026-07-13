use super::{
    ApplicationContainerPlan, ApplicationContainerPlanOptions, ApplicationContainerRequestOptions,
    ProjectRuntimeReconcileOptions, ProjectRuntimeReconcileResult, WorkloadReconcileError,
    WorkloadReconcileOptions, application_container_request, reconcile_project_application,
};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, EngineError, HealthObserver, ImageBuilder,
};

const PREFLIGHT_IMAGE_ID: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// Builds or reuses one runtime only after its complete application plan validates.
pub(crate) async fn reconcile_project_runtime<E>(
    engine: &mut E,
    options: ProjectRuntimeReconcileOptions<'_>,
) -> Result<ProjectRuntimeReconcileResult, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver + ImageBuilder,
{
    validate_ownership(&options)?;
    let plan = ApplicationContainerPlan::new(ApplicationContainerPlanOptions {
        project: options.project,
        image_digest: PREFLIGHT_IMAGE_ID.to_owned(),
        source_path: options.source_path,
        network_name: options.network_name,
        internal_http_port: options.internal_http_port,
    })
    .map_err(invalid_request)?;
    let route = plan.gateway_route().clone();
    let request = application_container_request(ApplicationContainerRequestOptions {
        plan,
        metadata: options.application_metadata,
        platform: options.platform,
        command: options.command,
        environment: options.environment,
    })
    .map_err(invalid_request)?;

    let runtime_image_id = engine
        .build_image(options.runtime_image.request())
        .await
        .map_err(|error| engine_error("build runtime image", error))?;
    let request = request
        .with_image(runtime_image_id.as_str())
        .map_err(invalid_request)?;
    let workload = reconcile_project_application(
        engine,
        WorkloadReconcileOptions {
            request: &request,
            installation_id: options.installation_id,
            schema_version: options.schema_version,
        },
    )
    .await?;

    Ok(ProjectRuntimeReconcileResult::new(
        runtime_image_id,
        route,
        workload,
    ))
}

fn validate_ownership(
    options: &ProjectRuntimeReconcileOptions<'_>,
) -> Result<(), WorkloadReconcileError> {
    let build_metadata = options.runtime_image.request().metadata();
    if build_metadata.installation_id() != options.installation_id
        || build_metadata.schema_version() != options.schema_version
    {
        return Err(WorkloadReconcileError::InvalidRequest {
            detail: "runtime image ownership does not match the reconciliation scope".to_owned(),
        });
    }
    if options.application_metadata.installation_id() != options.installation_id
        || options.application_metadata.schema_version() != options.schema_version
        || options.application_metadata.compatibility_fingerprint()
            != options.runtime_image.compatibility_fingerprint()
    {
        return Err(WorkloadReconcileError::InvalidRequest {
            detail: "application ownership does not match its runtime image identity".to_owned(),
        });
    }

    Ok(())
}

fn invalid_request(error: impl std::fmt::Display) -> WorkloadReconcileError {
    WorkloadReconcileError::InvalidRequest {
        detail: error.to_string(),
    }
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
