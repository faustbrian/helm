use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::workload::RuntimeImageBuildPlan;

/// Exact Engine mutation and route produced from one resolved application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableProjectApplicationPlan {
    request: ContainerCreateOptions,
    route: GatewayRoute,
    runtime_image: Option<RuntimeImageBuildPlan>,
}

impl ImmutableProjectApplicationPlan {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        route: GatewayRoute,
        runtime_image: Option<RuntimeImageBuildPlan>,
    ) -> Self {
        Self {
            request,
            route,
            runtime_image,
        }
    }

    pub(crate) const fn request(&self) -> &ContainerCreateOptions {
        &self.request
    }

    pub(crate) const fn route(&self) -> &GatewayRoute {
        &self.route
    }

    pub(crate) const fn runtime_image(&self) -> Option<&RuntimeImageBuildPlan> {
        self.runtime_image.as_ref()
    }
}
