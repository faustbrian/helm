/// Durable installation state controlling whether reconciliation may create data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallationLifecycle {
    Active,
    Deleting,
}

impl InstallationLifecycle {
    pub(super) fn from_label(label: &str) -> Option<Self> {
        match label {
            "active" => Some(Self::Active),
            "deleting" => Some(Self::Deleting),
            _ => None,
        }
    }
}
