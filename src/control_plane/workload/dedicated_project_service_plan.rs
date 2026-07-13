use crate::control_plane::engine::{ContainerCreateOptions, VolumeCreateOptions};

/// Exact container and optional retained data volume for one project service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DedicatedProjectServicePlan {
    request: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
}

impl DedicatedProjectServicePlan {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        volume: Option<VolumeCreateOptions>,
    ) -> Self {
        Self { request, volume }
    }

    pub(crate) const fn request(&self) -> &ContainerCreateOptions {
        &self.request
    }

    pub(crate) const fn volume(&self) -> Option<&VolumeCreateOptions> {
        self.volume.as_ref()
    }
}
