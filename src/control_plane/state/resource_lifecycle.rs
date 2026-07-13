/// Durable lifecycle state controlling recovery and garbage collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ResourceLifecycle {
    Active,
    Orphaned,
    Retained,
}

impl ResourceLifecycle {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Orphaned => "orphaned",
            Self::Retained => "retained",
        }
    }

    pub(super) fn from_label(label: &str) -> Option<Self> {
        match label {
            "active" => Some(Self::Active),
            "orphaned" => Some(Self::Orphaned),
            "retained" => Some(Self::Retained),
            _ => None,
        }
    }
}
