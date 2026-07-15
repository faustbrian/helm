use crate::control_plane::engine::{ContainerCreateOptions, VolumeCreateOptions};

/// Exact container and optional retained data volume for one project service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DedicatedProjectServicePlan {
    request: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    provisioning_job: Option<ContainerCreateOptions>,
}

impl DedicatedProjectServicePlan {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        volume: Option<VolumeCreateOptions>,
        provisioning_job: Option<ContainerCreateOptions>,
    ) -> Self {
        Self {
            request,
            volume,
            provisioning_job,
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
}
