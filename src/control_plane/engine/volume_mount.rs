use super::EngineError;

/// One Stackctl-owned Engine volume mounted into a Linux container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VolumeMount {
    source: String,
    target: String,
    read_only: bool,
}

impl VolumeMount {
    pub(crate) fn read_write(
        source: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self, EngineError> {
        Self::new(source.into(), target.into(), false)
    }

    pub(crate) fn read_only(
        source: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self, EngineError> {
        Self::new(source.into(), target.into(), true)
    }

    fn new(source: String, target: String, read_only: bool) -> Result<Self, EngineError> {
        if source.is_empty()
            || source.contains(['/', '\0'])
            || !target.starts_with('/')
            || target.contains('\0')
        {
            return Err(EngineError::InvalidRequest {
                detail: "volume mounts require a managed volume name and absolute Linux target"
                    .to_owned(),
            });
        }

        Ok(Self {
            source,
            target,
            read_only,
        })
    }

    pub(super) fn source(&self) -> &str {
        &self.source
    }

    pub(super) fn target(&self) -> &str {
        &self.target
    }

    pub(super) const fn is_read_only(&self) -> bool {
        self.read_only
    }
}
