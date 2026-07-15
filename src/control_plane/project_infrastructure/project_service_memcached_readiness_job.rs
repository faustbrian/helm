use super::{ProjectServicePreparationError, ProjectServiceProvisioningJob};
use crate::control_plane::project_infrastructure::project_service_provisioning_images::BUSYBOX_CLIENT_IMAGE;
use std::collections::BTreeMap;

/// Builds one bounded Memcached protocol readiness exchange.
pub(super) fn project_service_memcached_readiness_job(
    container_name: &str,
) -> Result<ProjectServiceProvisioningJob, ProjectServicePreparationError> {
    ProjectServiceProvisioningJob::new(
        BUSYBOX_CLIENT_IMAGE,
        vec![
            "sh".to_owned(),
            "-ec".to_owned(),
            format!(
                "printf 'version\\r\\n' | nc -w 5 {container_name} 11211 | grep -q '^VERSION '"
            ),
        ],
        BTreeMap::new(),
    )
}
