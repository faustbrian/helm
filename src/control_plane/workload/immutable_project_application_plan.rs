use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::gateway::GatewayRoute;

/// Exact Engine mutation and route produced from one resolved application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableProjectApplicationPlan {
    request: ContainerCreateOptions,
    route: GatewayRoute,
}

impl ImmutableProjectApplicationPlan {
    pub(super) const fn new(request: ContainerCreateOptions, route: GatewayRoute) -> Self {
        Self { request, route }
    }

    pub(crate) const fn request(&self) -> &ContainerCreateOptions {
        &self.request
    }

    pub(crate) const fn route(&self) -> &GatewayRoute {
        &self.route
    }
}
