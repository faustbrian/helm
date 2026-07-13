/// The owned purpose of a managed Engine resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ResourceKind {
    ProjectApplication,
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
            Self::SharedService => "shared_service",
            Self::Gateway => "gateway",
            Self::Network => "network",
            Self::Volume => "volume",
            Self::Build => "build",
        }
    }
}
