/// The owned purpose of a managed Engine resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ResourceKind {
    ProjectApplication,
    ProjectProcess,
    ProjectService,
    EphemeralService,
    SharedService,
    Gateway,
    Network,
    Volume,
    Build,
    ProvisioningJob,
}

impl ResourceKind {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::ProjectApplication => "project_application",
            Self::ProjectProcess => "project_process",
            Self::ProjectService => "project_service",
            Self::EphemeralService => "ephemeral_service",
            Self::SharedService => "shared_service",
            Self::Gateway => "gateway",
            Self::Network => "network",
            Self::Volume => "volume",
            Self::Build => "build",
            Self::ProvisioningJob => "provisioning_job",
        }
    }

    pub(crate) fn from_label(label: &str) -> Option<Self> {
        match label {
            "project_application" => Some(Self::ProjectApplication),
            "project_process" => Some(Self::ProjectProcess),
            "project_service" => Some(Self::ProjectService),
            "ephemeral_service" => Some(Self::EphemeralService),
            "shared_service" => Some(Self::SharedService),
            "gateway" => Some(Self::Gateway),
            "network" => Some(Self::Network),
            "volume" => Some(Self::Volume),
            "build" => Some(Self::Build),
            "provisioning_job" => Some(Self::ProvisioningJob),
            _ => None,
        }
    }
}
