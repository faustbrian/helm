use super::WorkloadPlanError;
use crate::control_plane::ServiceExecutionPlan;
use std::collections::BTreeMap;

/// Merges application and child-service values without hiding conflicts.
pub(crate) fn merge_project_declared_environment(
    service: &ServiceExecutionPlan,
    application: &ServiceExecutionPlan,
    workload: &str,
) -> Result<BTreeMap<String, String>, WorkloadPlanError> {
    let mut declared = application.desired().environment().clone();
    for (key, value) in service.desired().environment() {
        if declared.get(key).is_some_and(|existing| existing != value) {
            return Err(WorkloadPlanError::new(format!(
                "{workload} '{}-{}' environment key '{key}' conflicts with application '{}'",
                service.project().as_str(),
                service.service().as_str(),
                application.service().as_str()
            )));
        }
        declared.insert(key.clone(), value.clone());
    }

    Ok(declared)
}
