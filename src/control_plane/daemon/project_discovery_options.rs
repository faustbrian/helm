use super::ProjectDiscoveryError;

/// Explicit resource bounds for one correctness rescan of watched roots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProjectDiscoveryOptions {
    maximum_depth: usize,
    maximum_entries: usize,
    maximum_config_bytes: usize,
}

impl ProjectDiscoveryOptions {
    pub(crate) fn new(
        maximum_depth: usize,
        maximum_entries: usize,
        maximum_config_bytes: usize,
    ) -> Result<Self, ProjectDiscoveryError> {
        if maximum_entries == 0 || maximum_config_bytes == 0 {
            return Err(ProjectDiscoveryError::InvalidOptions);
        }

        Ok(Self {
            maximum_depth,
            maximum_entries,
            maximum_config_bytes,
        })
    }

    pub(crate) const fn bounded_defaults() -> Self {
        Self {
            maximum_depth: 16,
            maximum_entries: 100_000,
            maximum_config_bytes: 1024 * 1024,
        }
    }

    pub(super) const fn maximum_depth(self) -> usize {
        self.maximum_depth
    }

    pub(super) const fn maximum_entries(self) -> usize {
        self.maximum_entries
    }

    pub(super) const fn maximum_config_bytes(self) -> usize {
        self.maximum_config_bytes
    }
}
