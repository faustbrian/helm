/// Injection state for one retained managed environment revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnvironmentLifecycle {
    Active,
    Disabled,
}

impl EnvironmentLifecycle {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    pub(super) fn from_label(label: &str) -> Option<Self> {
        match label {
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}
