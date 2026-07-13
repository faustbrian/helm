use super::StateStoreError;

/// Durable lifecycle of one asynchronous daemon operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DaemonOperationStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl DaemonOperationStatus {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub(super) fn parse(value: &str) -> Result<Self, StateStoreError> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            value => Err(StateStoreError::CorruptState {
                detail: format!("daemon operation has unknown status '{value}'"),
            }),
        }
    }
}
