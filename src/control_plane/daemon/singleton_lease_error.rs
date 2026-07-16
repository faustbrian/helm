use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A per-user daemon lease acquisition failure.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum SingletonLeaseError {
    /// Another live process holds the operating-system lock.
    AlreadyRunning {
        path: PathBuf,
        owner_pid: Option<u32>,
    },
    /// The lease file could not be opened, locked, secured, or updated.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl Display for SingletonLeaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning { path, owner_pid } => match owner_pid {
                Some(owner_pid) => write!(
                    formatter,
                    "another Stackctl daemon owns '{}' (PID {owner_pid}); inspect it with \
                     `stackctl daemon service status`; stop an installed service with \
                     `stackctl daemon service uninstall --keep-data`, or terminate foreground \
                     PID {owner_pid} before retrying",
                    path.display()
                ),
                None => write!(
                    formatter,
                    "another Stackctl daemon owns '{}' (PID unavailable); inspect it with \
                     `stackctl daemon service status`; stop an installed service with \
                     `stackctl daemon service uninstall --keep-data`, or terminate the \
                     foreground daemon before retrying",
                    path.display()
                ),
            },
            Self::Io { path, source } => write!(
                formatter,
                "failed to acquire Stackctl daemon lease '{}': {source}",
                path.display()
            ),
        }
    }
}

impl Error for SingletonLeaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::AlreadyRunning { .. } => None,
            Self::Io { source, .. } => Some(source),
        }
    }
}
