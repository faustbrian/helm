#[cfg(test)]
use super::ProjectDiscoveryError;
use crate::control_plane::MAX_PROJECT_CONFIG_BYTES;

/// Explicit resource bounds for one correctness rescan of watched roots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProjectDiscoveryOptions {
    maximum_depth: usize,
    maximum_directories: usize,
    maximum_config_bytes: usize,
}

impl ProjectDiscoveryOptions {
    #[cfg(test)]
    pub(crate) fn new(
        maximum_depth: usize,
        maximum_directories: usize,
        maximum_config_bytes: usize,
    ) -> Result<Self, ProjectDiscoveryError> {
        if maximum_directories == 0 || maximum_config_bytes == 0 {
            return Err(ProjectDiscoveryError::InvalidOptions);
        }

        Ok(Self {
            maximum_depth,
            maximum_directories,
            maximum_config_bytes,
        })
    }

    pub(crate) const fn bounded_defaults() -> Self {
        Self {
            maximum_depth: 2,
            maximum_directories: 100_000,
            maximum_config_bytes: MAX_PROJECT_CONFIG_BYTES,
        }
    }

    pub(super) const fn maximum_depth(self) -> usize {
        self.maximum_depth
    }

    pub(super) const fn maximum_directories(self) -> usize {
        self.maximum_directories
    }

    pub(super) const fn maximum_config_bytes(self) -> usize {
        self.maximum_config_bytes
    }
}
