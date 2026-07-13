use super::{EngineError, ManagedResourceMetadata};

/// Typed request for one Stackctl-owned user-defined Engine network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NetworkCreateOptions {
    name: String,
    metadata: ManagedResourceMetadata,
}

impl NetworkCreateOptions {
    pub(crate) fn new(
        name: impl Into<String>,
        metadata: ManagedResourceMetadata,
    ) -> Result<Self, EngineError> {
        let name = name.into();

        if name.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: "managed network name must not be empty".to_owned(),
            });
        }

        Ok(Self { name, metadata })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn metadata(&self) -> &ManagedResourceMetadata {
        &self.metadata
    }
}
