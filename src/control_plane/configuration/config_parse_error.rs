use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A strict v8 configuration parsing or schema error.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ConfigParseError {
    path: PathBuf,
    detail: String,
}

impl ConfigParseError {
    pub(super) fn new(path: PathBuf, detail: impl Into<String>) -> Self {
        Self {
            path,
            detail: detail.into(),
        }
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
