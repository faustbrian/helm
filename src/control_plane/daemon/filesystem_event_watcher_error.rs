use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

/// A native watched-root setup or delivery failure.
#[derive(Debug)]
pub(crate) enum FilesystemEventWatcherError {
    Backend {
        action: &'static str,
        path: Option<PathBuf>,
        source: notify::Error,
    },
    Disconnected,
}

impl FilesystemEventWatcherError {
    pub(super) fn start(source: notify::Error) -> Self {
        Self::Backend {
            action: "start native filesystem watcher",
            path: None,
            source,
        }
    }

    pub(super) fn watch(path: &Path, source: notify::Error) -> Self {
        Self::Backend {
            action: "watch root",
            path: Some(path.to_path_buf()),
            source,
        }
    }

    pub(super) const fn disconnected() -> Self {
        Self::Disconnected
    }
}

impl Display for FilesystemEventWatcherError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend {
                action,
                path: Some(path),
                source,
            } => write!(
                formatter,
                "failed to {action} '{}': {source}",
                path.display()
            ),
            Self::Backend {
                action,
                path: None,
                source,
            } => write!(formatter, "failed to {action}: {source}"),
            Self::Disconnected => formatter.write_str("native filesystem watcher disconnected"),
        }
    }
}

impl Error for FilesystemEventWatcherError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Backend { source, .. } => Some(source),
            Self::Disconnected => None,
        }
    }
}
