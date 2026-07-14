use std::error::Error;
use std::fmt::{Display, Formatter};

/// A deterministic failure to prove one exact v7 project inventory.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct V7InventoryError {
    detail: String,
}

impl V7InventoryError {
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for V7InventoryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for V7InventoryError {}
