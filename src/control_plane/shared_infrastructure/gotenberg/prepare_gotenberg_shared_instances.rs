use super::{
    GotenbergPreparationError, GotenbergPreparationOptions, GotenbergSharedInstancePlan,
    GotenbergSharedInstancePlanOptions, PreparedGotenbergSharedInstance,
    plan_gotenberg_project_resources,
};
use crate::control_plane::shared_infrastructure::SharedInstancePlan;

/// Materializes stateless instances and project endpoint environments.
pub(crate) fn prepare_gotenberg_shared_instances(
    shared_instances: &[SharedInstancePlan],
    options: GotenbergPreparationOptions<'_>,
) -> Result<Vec<PreparedGotenbergSharedInstance>, GotenbergPreparationError> {
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if shared.profile().implementation() != "gotenberg" {
            return Err(invalid(format!(
                "Gotenberg preparation cannot materialize implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let instance = GotenbergSharedInstancePlan::new(
            shared,
            GotenbergSharedInstancePlanOptions {
                installation_id: options.installation_id.to_owned(),
                network_name: options.network_name.to_owned(),
                schema_version: options.schema_version,
                desired_revision: shared.fingerprint().as_str().to_owned(),
            },
        )
        .map_err(invalid)?;
        let projects = shared
            .consumers()
            .iter()
            .map(|consumer| {
                plan_gotenberg_project_resources(
                    consumer.project_id(),
                    consumer.service_id(),
                    &instance,
                )
                .map_err(invalid)
            })
            .collect::<Result<Vec<_>, _>>()?;
        prepared.push(PreparedGotenbergSharedInstance::new(instance, projects));
    }

    Ok(prepared)
}

fn invalid(error: impl std::fmt::Display) -> GotenbergPreparationError {
    GotenbergPreparationError::new(error.to_string())
}
