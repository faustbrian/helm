/// An opaque container identifier returned by an Engine backend.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ContainerId(String);

impl ContainerId {
    /// Wraps an opaque backend identifier without interpreting it.
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the exact backend identifier.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
