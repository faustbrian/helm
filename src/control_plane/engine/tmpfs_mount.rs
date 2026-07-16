use super::EngineError;

/// Bounded in-memory filesystem for disposable container state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TmpfsMount {
    target: String,
    options: String,
}

impl TmpfsMount {
    pub(crate) fn new(target: impl Into<String>, size_bytes: u64) -> Result<Self, EngineError> {
        let target = target.into();
        if !target.starts_with('/') || target.contains('\0') {
            return Err(EngineError::InvalidRequest {
                detail: format!("tmpfs target '{target}' must be an absolute container path"),
            });
        }
        if size_bytes == 0 || size_bytes > i64::MAX as u64 {
            return Err(EngineError::InvalidRequest {
                detail: "tmpfs size must fit a positive Engine byte range".to_owned(),
            });
        }

        Ok(Self {
            target,
            options: format!("rw,noexec,nosuid,mode=1777,size={size_bytes}"),
        })
    }

    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) fn options(&self) -> &str {
        &self.options
    }
}
