use super::EngineError;

/// Typed history window requested before optionally following container logs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContainerLogTail {
    All,
    Last(u32),
}

impl ContainerLogTail {
    pub(crate) const fn all() -> Self {
        Self::All
    }

    pub(crate) fn last(lines: u32) -> Result<Self, EngineError> {
        if lines == 0 {
            return Err(EngineError::InvalidRequest {
                detail: "container log tail must be greater than zero".to_owned(),
            });
        }

        Ok(Self::Last(lines))
    }

    pub(super) fn engine_value(self) -> String {
        match self {
            Self::All => "all".to_owned(),
            Self::Last(lines) => lines.to_string(),
        }
    }
}
