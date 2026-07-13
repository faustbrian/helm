/// Opaque Engine execution identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommandExecutionId(String);

impl CommandExecutionId {
    pub(crate) fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
