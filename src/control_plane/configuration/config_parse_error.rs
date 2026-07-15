use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A strict v8 configuration parsing or schema error.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ConfigParseError {
    path: PathBuf,
    detail: String,
    security_policy_blocked: bool,
}

impl ConfigParseError {
    pub(super) fn new(path: PathBuf, detail: impl Into<String>) -> Self {
        Self {
            path,
            detail: detail.into(),
            security_policy_blocked: false,
        }
    }

    pub(super) fn security_policy_blocked(path: PathBuf, detail: impl Into<String>) -> Self {
        Self {
            path,
            detail: detail.into(),
            security_policy_blocked: true,
        }
    }

    pub(crate) const fn is_security_policy_blocked(&self) -> bool {
        self.security_policy_blocked
    }
}

impl Display for ConfigParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid v8 configuration '{}': {}",
            self.path.display(),
            self.detail
        )
    }
}

impl Error for ConfigParseError {}
