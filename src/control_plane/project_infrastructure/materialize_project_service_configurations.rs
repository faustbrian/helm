use super::{PreparedProjectService, ProjectServicePreparationError};
use crate::control_plane::engine::BindMount;
use crate::control_plane::shared_infrastructure::store_credential_secret;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Atomically stores generated service files and binds only their exact paths.
#[cfg(unix)]
pub(crate) fn materialize_project_service_configurations(
    services: &mut [PreparedProjectService],
    state_directory: &Path,
) -> Result<(), ProjectServicePreparationError> {
    for service in services {
        let Some(configuration) = service.container_configuration() else {
            continue;
        };
        let directory = state_directory
            .join("project-services")
            .join(service.project_id())
            .join(service.service_id())
            .join("configurations")
            .join(hex::encode(Sha256::digest(
                configuration.contents().expose().as_bytes(),
            )));
        let path = directory.join(configuration.file_name());
        let stored = store_credential_secret(configuration.contents(), &path).map_err(invalid)?;
        let source = stored.to_str().ok_or_else(|| {
            invalid(format!(
                "project service configuration path '{}' is not valid UTF-8",
                stored.display()
            ))
        })?;
        let mount = BindMount::read_only(source, configuration.mount_target()).map_err(invalid)?;
        service.set_container_configuration_mount(mount);
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
