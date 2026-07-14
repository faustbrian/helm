use crate::control_plane::ServiceIdentity;
use crate::control_plane::engine::ContainerCreateOptions;

/// One project process bound to its exact application runtime identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectProcessOperationPlan {
    request: ContainerCreateOptions,
    application_service: ServiceIdentity,
}

impl ProjectProcessOperationPlan {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        application_service: ServiceIdentity,
    ) -> Self {
        Self {
            request,
            application_service,
        }
    }

    pub(crate) const fn request(&self) -> &ContainerCreateOptions {
        &self.request
    }

    pub(crate) fn application_service(&self) -> &str {
        self.application_service.as_str()
    }
}
