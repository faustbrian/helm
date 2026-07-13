/// Opaque content-addressed Engine image configuration identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImageId(String);

impl ImageId {
    pub(crate) fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
