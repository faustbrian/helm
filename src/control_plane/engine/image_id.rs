use super::EngineError;

/// Opaque content-addressed Engine image configuration identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImageId(String);

impl ImageId {
    pub(crate) fn new(id: impl Into<String>) -> Result<Self, EngineError> {
        let id = id.into();
        let valid = id.strip_prefix("sha256:").is_some_and(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
        if !valid {
            return Err(EngineError::Backend {
                detail: format!("Engine image ID '{id}' must be a sha256 content identity"),
            });
        }

        Ok(Self(id))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
