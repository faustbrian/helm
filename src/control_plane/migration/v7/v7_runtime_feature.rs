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

impl V7RuntimeFeature {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Hooks => "hooks",
            Self::PhpExtensions => "php_extensions",
            Self::CustomCommand => "custom_command",
            Self::CustomEnvironment => "custom_environment",
            Self::EnvironmentMapping => "environment_mapping",
            Self::HealthCheck => "health_check",
            Self::JavaScript => "javascript",
            Self::LocalhostTls => "localhost_tls",
            Self::Octane => "octane",
            Self::SeedFile => "seed_file",
            Self::RestartPolicy => "restart_policy",
        }
    }
}
