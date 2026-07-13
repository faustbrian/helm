use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A watched-root failure that prevents a trustworthy complete rescan.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum ProjectDiscoveryError {
    InvalidOptions,
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    RootNotDirectory {
        path: PathBuf,
    },
    EntryLimit {
        maximum: usize,
    },
}

impl Display for ProjectDiscoveryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOptions => write!(
                formatter,
                "project discovery requires non-zero entry and config byte limits"
            ),
            Self::Io {
                action,
                path,
                source,
            } => write!(
                formatter,
                "failed to {action} '{}': {source}",
                path.display()
            ),
            Self::RootNotDirectory { path } => write!(
                formatter,
                "watched root '{}' must be a directory",
                path.display()
            ),
            Self::EntryLimit { maximum } => write!(
                formatter,
                "project discovery exceeded the configured {maximum} entry limit"
            ),
        }
    }
}

impl Error for ProjectDiscoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidOptions | Self::RootNotDirectory { .. } | Self::EntryLimit { .. } => None,
        }
    }
}
