/// Container Engine implementation selected during Stackctl installation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum EngineProvider {
    Docker,
}

impl EngineProvider {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Docker => "docker",
        }
    }

    pub(super) fn from_label(label: &str) -> Option<Self> {
        match label {
            "docker" => Some(Self::Docker),
            _ => None,
        }
    }
}
