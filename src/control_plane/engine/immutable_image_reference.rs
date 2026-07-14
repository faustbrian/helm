use super::EngineError;

/// Registry image reference pinned to an immutable sha256 manifest digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableImageReference(String);

impl ImmutableImageReference {
    pub(crate) fn new(reference: impl Into<String>) -> Result<Self, EngineError> {
        let reference = reference.into();

        if !has_sha256_digest(&reference) {
            return Err(EngineError::InvalidRequest {
                detail: format!("managed image '{reference}' must use an immutable sha256 digest"),
            });
        }

        Ok(Self(reference))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

fn has_sha256_digest(image: &str) -> bool {
    let Some((repository, digest)) = image.rsplit_once("@sha256:") else {
        return false;
    };

    valid_repository(repository)
        && digest.len() == 64
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_repository(repository: &str) -> bool {
    repository
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && repository.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
        })
}
