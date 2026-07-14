/// Secret-free marker for one customized v7 runtime behavior.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum V7RuntimeFeature {
    Hooks,
    PhpExtensions,
    CustomCommand,
    CustomEnvironment,
    EnvironmentMapping,
    HealthCheck,
    JavaScript,
    LocalhostTls,
    Octane,
    SeedFile,
    RestartPolicy,
}
