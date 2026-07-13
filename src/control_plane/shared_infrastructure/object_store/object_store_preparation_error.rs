use std::fmt::{Display, Formatter};

/// Durable credential or object-store materialization failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ObjectStorePreparationError {
    detail: String,
}

impl ObjectStorePreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for ObjectStorePreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for ObjectStorePreparationError {}
