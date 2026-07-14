/// Verified target identity produced by one selected v7 adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum V7MigrationAdapterTarget {
    Resource(String),
    NoExternalTarget,
}

impl V7MigrationAdapterTarget {
    pub(crate) fn resource(reference: impl Into<String>) -> Result<Self, String> {
        let reference = reference.into();
        if reference.is_empty() || reference.contains('\0') {
            return Err("v7 migration target reference must be non-empty".to_owned());
        }

        Ok(Self::Resource(reference))
    }

    pub(crate) const fn reference(&self) -> Option<&str> {
        match self {
            Self::Resource(reference) => Some(reference.as_str()),
            Self::NoExternalTarget => None,
        }
    }
}
