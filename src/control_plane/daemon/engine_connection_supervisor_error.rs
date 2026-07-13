use std::fmt::{Display, Formatter};

/// Invalid immutable Engine connection supervision options.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct EngineConnectionSupervisorError {
    detail: &'static str,
}

impl EngineConnectionSupervisorError {
    pub(super) const fn new(detail: &'static str) -> Self {
        Self { detail }
    }
}

impl Display for EngineConnectionSupervisorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for EngineConnectionSupervisorError {}
