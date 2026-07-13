use super::{GotenbergPlanError, GotenbergProjectResources, GotenbergSharedInstancePlan};
use crate::control_plane::DnsLabel;
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes the stable internal endpoint for one Gotenberg consumer.
pub(crate) fn plan_gotenberg_project_resources(
    project_id: &str,
    instance: &GotenbergSharedInstancePlan,
) -> Result<GotenbergProjectResources, GotenbergPlanError> {
    let project_id = DnsLabel::new("project", project_id)
        .map_err(|error| GotenbergPlanError::new(error.to_string()))?;
    let values = BTreeMap::from([(
        "GOTENBERG_URL".to_owned(),
        format!("http://{}:3000", instance.container().name()),
    )]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        GotenbergPlanError::new(format!(
            "failed to encode Gotenberg managed environment: {error}"
        ))
    })?;
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(canonical)));
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.as_str().to_owned(),
        revision,
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(GotenbergProjectResources::new(environment))
}
