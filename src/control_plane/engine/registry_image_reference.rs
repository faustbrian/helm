use super::{EngineError, ImmutableImageReference};

/// One exact mutable registry reference awaiting manifest resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RegistryImageReference {
    source: String,
    repository: String,
}

impl RegistryImageReference {
    pub(crate) fn new(source: impl Into<String>) -> Result<Self, EngineError> {
        let source = source.into();
        if source.is_empty() || source.chars().any(char::is_whitespace) || source.contains('@') {
            return Err(invalid_reference(&source));
        }

        let last_slash = source.rfind('/');
        let last_colon = source.rfind(':');
        let repository =
            match last_colon.filter(|colon| last_slash.is_none_or(|slash| colon > &slash)) {
                Some(colon) => {
                    let (repository, tag) = source.split_at(colon);
                    if repository.is_empty() || tag.len() == 1 {
                        return Err(invalid_reference(&source));
                    }
                    repository
                }
                None => source.as_str(),
            }
            .to_owned();
        if repository.is_empty()
            || repository.starts_with('/')
            || repository.ends_with('/')
            || repository.contains("//")
        {
            return Err(invalid_reference(&source));
        }

        Ok(Self { source, repository })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.source
    }

    #[cfg(test)]
    pub(crate) fn repository(&self) -> &str {
        &self.repository
    }

    pub(crate) fn with_digest(
        &self,
        digest: impl AsRef<str>,
    ) -> Result<ImmutableImageReference, EngineError> {
        let digest = digest.as_ref();
        ImmutableImageReference::new(format!("{}@{digest}", self.repository))
    }
}

fn invalid_reference(source: &str) -> EngineError {
    EngineError::InvalidRequest {
        detail: format!(
            "registry image reference '{source}' must be an exact non-digest repository with an optional tag"
        ),
    }
}
