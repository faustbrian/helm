use super::{EngineError, ManagedResourceMetadata};

/// Typed options required to create one owned container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContainerCreateOptions {
    name: String,
    image: String,
    metadata: ManagedResourceMetadata,
}

impl ContainerCreateOptions {
    /// Creates options only for an immutable sha256 image reference.
    pub(crate) fn new(
        name: impl Into<String>,
        image: impl Into<String>,
        metadata: ManagedResourceMetadata,
    ) -> Result<Self, EngineError> {
        let name = name.into();
        let image = image.into();

        if name.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: "managed container name must not be empty".to_owned(),
            });
        }

        if !has_sha256_digest(&image) {
            return Err(EngineError::InvalidRequest {
                detail: format!("managed image '{image}' must use an immutable sha256 digest"),
            });
        }

        Ok(Self {
            name,
            image,
            metadata,
        })
    }

    /// Returns the exact engine resource name.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Returns the immutable image reference.
    pub(crate) fn image(&self) -> &str {
        &self.image
    }

    /// Returns mandatory ownership metadata.
    pub(crate) const fn metadata(&self) -> &ManagedResourceMetadata {
        &self.metadata
    }
}

fn has_sha256_digest(image: &str) -> bool {
    let Some((_, digest)) = image.rsplit_once("@sha256:") else {
        return false;
    };

    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}
