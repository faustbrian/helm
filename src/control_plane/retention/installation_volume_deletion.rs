use crate::control_plane::state::{RecoveryPointRecord, ResourceRecord};
use sha2::{Digest, Sha256};

/// Secret-free recovery authorization for one persistent project volume.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstallationVolumeDeletion {
    resource_id: String,
    project_id: String,
    service_id: String,
    recovery_point_id: String,
    confirmation_token: String,
}

impl InstallationVolumeDeletion {
    pub(crate) fn new(
        resource: &ResourceRecord,
        recovery: &RecoveryPointRecord,
    ) -> Result<Self, String> {
        let project_id = resource
            .project_id()
            .ok_or_else(|| "installation volume deletion requires a project owner".to_owned())?;
        let service_id = resource
            .scope_id()
            .ok_or_else(|| "installation volume deletion requires a service owner".to_owned())?;
        let exact = resource.kind() == "volume"
            && recovery.project_id() == project_id
            && recovery.service_id() == service_id
            && recovery.logical_resource_id() == resource.resource_id()
            && recovery.resource_kind() == resource.kind()
            && recovery.compatibility_fingerprint() == resource.compatibility_fingerprint();
        if !exact {
            return Err(
                "installation volume deletion recovery does not match exact ownership".to_owned(),
            );
        }
        let confirmation_token = token(resource, recovery, project_id, service_id);

        Ok(Self {
            resource_id: resource.resource_id().to_owned(),
            project_id: project_id.to_owned(),
            service_id: service_id.to_owned(),
            recovery_point_id: recovery.recovery_point_id().to_owned(),
            confirmation_token,
        })
    }

    pub(crate) fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn recovery_point_id(&self) -> &str {
        &self.recovery_point_id
    }

    pub(crate) fn confirmation_token(&self) -> &str {
        &self.confirmation_token
    }
}

fn token(
    resource: &ResourceRecord,
    recovery: &RecoveryPointRecord,
    project_id: &str,
    service_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    for field in [
        "stackctl-installation-volume-delete-v1".to_owned(),
        resource.installation_id().to_owned(),
        resource.resource_id().to_owned(),
        project_id.to_owned(),
        service_id.to_owned(),
        resource.compatibility_fingerprint().to_owned(),
        recovery.recovery_point_id().to_owned(),
        recovery.artifact_sha256().to_owned(),
        recovery.artifact_size_bytes().to_string(),
        recovery.created_at_unix_seconds().to_string(),
        recovery.verified_at_unix_seconds().to_string(),
    ] {
        hasher.update(field.len().to_be_bytes());
        hasher.update(field.as_bytes());
    }

    hex::encode(hasher.finalize())
}
