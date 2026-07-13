use serde::Serialize;

/// One value-redacted semantic difference requiring user review.
#[derive(Clone, Debug, Serialize)]
pub struct MigrationDifference {
    path: String,
    detail: String,
    blocking: bool,
}

impl MigrationDifference {
    pub(super) fn new(path: impl Into<String>, detail: impl Into<String>, blocking: bool) -> Self {
        Self {
            path: path.into(),
            detail: detail.into(),
            blocking,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub const fn blocking(&self) -> bool {
        self.blocking
    }
}
