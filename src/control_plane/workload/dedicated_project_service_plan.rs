use crate::control_plane::engine::{ContainerCreateOptions, VolumeCreateOptions};

/// Exact container and optional retained data volume for one project service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DedicatedProjectServicePlan {
    request: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    provisioning_job: Option<ContainerCreateOptions>,
    authentication_failure_exit_status: Option<i64>,
}

impl DedicatedProjectServicePlan {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        volume: Option<VolumeCreateOptions>,
        provisioning_job: Option<ContainerCreateOptions>,
        authentication_failure_exit_status: Option<i64>,
    ) -> Self {
        Self {
            request,
            volume,
            provisioning_job,
            authentication_failure_exit_status,
        }
    }

    pub(crate) const fn request(&self) -> &ContainerCreateOptions {
        &self.request
    }

    pub(crate) const fn volume(&self) -> Option<&VolumeCreateOptions> {
        self.volume.as_ref()
    }

    pub(crate) const fn provisioning_job(&self) -> Option<&ContainerCreateOptions> {
        self.provisioning_job.as_ref()
    }

    pub(crate) const fn authentication_failure_exit_status(&self) -> Option<i64> {
        self.authentication_failure_exit_status
    }
}
