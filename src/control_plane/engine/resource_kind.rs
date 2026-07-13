/// The owned purpose of a managed Engine resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ResourceKind {
    ProjectApplication,
    ProjectProcess,
    SharedService,
    Gateway,
    Network,
    Volume,
    Build,
}

impl ResourceKind {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::ProjectApplication => "project_application",
            Self::ProjectProcess => "project_process",
            Self::SharedService => "shared_service",
            Self::Gateway => "gateway",
            Self::Network => "network",
            Self::Volume => "volume",
            Self::Build => "build",
        }
    }

    pub(super) fn from_label(label: &str) -> Option<Self> {
        match label {
            "project_application" => Some(Self::ProjectApplication),
            "project_process" => Some(Self::ProjectProcess),
            "shared_service" => Some(Self::SharedService),
            "gateway" => Some(Self::Gateway),
            "network" => Some(Self::Network),
            "volume" => Some(Self::Volume),
            "build" => Some(Self::Build),
            _ => None,
        }
    }
}
