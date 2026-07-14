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
    Ephemeral,
}

impl ServiceDeploymentStrategy {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::SharedByCompatibility => "shared-by-compatibility",
            Self::SharedWithAttribution => "shared-with-attribution",
            Self::SharedStateless => "shared-stateless",
            Self::DedicatedProject => "dedicated-project",
            Self::DedicatedUntilIsolationProven => "dedicated-until-isolation-proven",
            Self::ProjectApplication => "project-application",
            Self::ProjectProcess => "project-process",
            Self::Ephemeral => "ephemeral",
        }
    }

    /// Whether this strategy produces one deterministic user-facing HTTP route.
    pub(crate) const fn claims_gateway_route(self) -> bool {
        matches!(self, Self::ProjectApplication | Self::SharedWithAttribution)
    }
}
