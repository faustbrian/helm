use crate::control_plane::retention::InstallationVolumeDeletion;
use serde::{Deserialize, Serialize};

/// Secret-free persistent-volume recovery authorization in a teardown plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcInstallationVolumeDeletion {
    resource_id: String,
    project_id: String,
    service_id: String,
    recovery_point_id: String,
}

impl IpcInstallationVolumeDeletion {
    #[cfg(test)]
    pub(crate) fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[cfg(test)]
    pub(crate) fn recovery_point_id(&self) -> &str {
        &self.recovery_point_id
    }
}

impl From<&InstallationVolumeDeletion> for IpcInstallationVolumeDeletion {
    fn from(deletion: &InstallationVolumeDeletion) -> Self {
        Self {
            resource_id: deletion.resource_id().to_owned(),
            project_id: deletion.project_id().to_owned(),
            service_id: deletion.service_id().to_owned(),
            recovery_point_id: deletion.recovery_point_id().to_owned(),
        }
    }
}
