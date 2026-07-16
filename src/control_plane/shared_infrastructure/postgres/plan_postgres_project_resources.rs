use super::{
    PostgresPlanError, PostgresProjectResources, PostgresSharedInstancePlan,
    plan_postgres_project_resources_for_host,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;

/// Composes logical SQL, stable credential state, and application environment.
pub(crate) fn plan_postgres_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &PostgresSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<PostgresProjectResources, PostgresPlanError> {
    plan_postgres_project_resources_for_host(
        project_id,
        service_id,
        instance.container().name(),
        secret,
    )
}
