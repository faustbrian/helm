/// Durable deletion class independent of a particular Engine adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ResourceRetention {
    Persistent,
    Disposable,
    BuildCache,
}

impl ResourceRetention {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::Disposable => "disposable",
            Self::BuildCache => "build_cache",
        }
    }

    pub(super) fn from_label(label: &str) -> Option<Self> {
        match label {
            "persistent" => Some(Self::Persistent),
            "disposable" => Some(Self::Disposable),
            "build_cache" => Some(Self::BuildCache),
            _ => None,
        }
    }
}
