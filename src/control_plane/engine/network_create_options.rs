use super::{EngineError, ManagedResourceMetadata};

/// Typed request for one Stackctl-owned user-defined Engine network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NetworkCreateOptions {
    name: String,
    metadata: ManagedResourceMetadata,
    subnet: Option<String>,
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

        Ok(Self {
            name,
            metadata,
            subnet: None,
        })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn metadata(&self) -> &ManagedResourceMetadata {
        &self.metadata
    }

    pub(crate) fn with_subnet(mut self, subnet: impl Into<String>) -> Self {
        self.subnet = Some(subnet.into());
        self
    }

    pub(crate) fn subnet(&self) -> Option<&str> {
        self.subnet.as_deref()
    }
}
