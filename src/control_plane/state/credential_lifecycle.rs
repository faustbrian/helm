/// Durable usability state for one project-scoped credential.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CredentialLifecycle {
    Active,
    Disabled,
}

impl CredentialLifecycle {
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
