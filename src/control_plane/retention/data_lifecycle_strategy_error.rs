use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DataLifecycleStrategyError {
    NonAuthoritative { kind: String },
    UnknownKind { kind: String },
}

impl Display for DataLifecycleStrategyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonAuthoritative { kind } => write!(
                formatter,
                "logical resource kind '{kind}' has no authoritative data to back up"
            ),
            Self::UnknownKind { kind } => write!(
                formatter,
                "logical resource kind '{kind}' has no registered data lifecycle strategy"
            ),
        }
    }
}

impl std::error::Error for DataLifecycleStrategyError {}
