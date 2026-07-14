use super::{PreparedProjectService, ProjectServicePreparationError};
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes one credential-free Memcached endpoint for a project runtime.
pub(crate) fn plan_memcached_project_resources(
    service: &ServiceExecutionPlan,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("memcached") {
        return Err(invalid(format!(
            "Memcached preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let values = BTreeMap::from([
        (
            "MEMCACHED_HOST".to_owned(),
            format!("stackctl-{project_id}-{service_id}"),
        ),
        ("MEMCACHED_PORT".to_owned(), "11211".to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(invalid)?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(PreparedProjectService::new(
        project_id.to_owned(),
        service_id.to_owned(),
        None,
        environment,
        BTreeMap::new(),
        None,
    ))
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
