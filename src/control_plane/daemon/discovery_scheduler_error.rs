use std::fmt::{Display, Formatter};

/// Invalid timing policy that could create a hot loop or unbounded settling.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DiscoverySchedulerError {
    detail: &'static str,
}

impl DiscoverySchedulerError {
    pub(super) const fn new(detail: &'static str) -> Self {
        Self { detail }
    }
}

impl Display for DiscoverySchedulerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for DiscoverySchedulerError {}
