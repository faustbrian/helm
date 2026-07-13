use super::EngineError;

/// One explicit host path mounted into a managed Linux container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BindMount {
    source: String,
    target: String,
    read_only: bool,
}

impl BindMount {
    pub(crate) fn read_only(
        source: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let source = source.into();
        let target = target.into();

        if source.is_empty() || !target.starts_with('/') {
            return Err(EngineError::InvalidRequest {
                detail: "bind mounts require a host source and absolute Linux target".to_owned(),
            });
        }

        Ok(Self {
            source,
            target,
            read_only: true,
        })
    }

    pub(crate) fn read_write(
        source: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let mut mount = Self::read_only(source, target)?;
        mount.read_only = false;

        Ok(mount)
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) const fn is_read_only(&self) -> bool {
        self.read_only
    }
}
