/// Safe default workload scope for one recognized v8 preset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServiceDeploymentStrategy {
    SharedByCompatibility,
    SharedWithAttribution,
    SharedStateless,
    DedicatedProject,
    DedicatedUntilIsolationProven,
    ProjectApplication,
    ProjectProcess,
    ProjectScheduledCommand,
    Ephemeral,
}

impl ServiceDeploymentStrategy {
    /// Whether this strategy produces one deterministic user-facing HTTP route.
    pub(crate) const fn claims_gateway_route(self) -> bool {
        matches!(self, Self::ProjectApplication | Self::SharedWithAttribution)
    }
}
