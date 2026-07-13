use std::error::Error;
use std::fmt::{Display, Formatter};

/// Invalid object-store compatibility or logical-isolation input.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ObjectStorePlanError {
    detail: String,
}

impl ObjectStorePlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for ObjectStorePlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for ObjectStorePlanError {}
