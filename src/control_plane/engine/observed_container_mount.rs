/// Backend-independent mount observed on a running or stopped container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObservedContainerMount {
    source: String,
    target: String,
    named_volume: bool,
    read_only: bool,
}

impl ObservedContainerMount {
    pub(crate) fn new(
        source: impl Into<String>,
        target: impl Into<String>,
        named_volume: bool,
        read_only: bool,
    ) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            named_volume,
            read_only,
        }
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) const fn is_named_volume(&self) -> bool {
        self.named_volume
    }

    pub(crate) const fn is_read_only(&self) -> bool {
        self.read_only
    }
}
