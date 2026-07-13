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
