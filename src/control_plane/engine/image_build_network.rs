/// Network policy for one derived Engine image build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageBuildNetwork {
    Disabled,
    Enabled,
}

impl ImageBuildNetwork {
    pub(crate) const fn engine_mode(self) -> Option<&'static str> {
        match self {
            Self::Disabled => Some("none"),
            Self::Enabled => None,
        }
    }

    pub(crate) const fn fingerprint(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Enabled => "enabled",
        }
    }
}
