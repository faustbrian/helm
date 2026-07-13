/// Opaque Engine identity for one managed network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NetworkId(String);

impl NetworkId {
    pub(crate) fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
