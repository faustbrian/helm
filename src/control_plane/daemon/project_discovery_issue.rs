use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A project-scoped discovery problem that does not block unrelated projects.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ProjectDiscoveryIssue {
    ConfigTooLarge {
        path: PathBuf,
        actual: u64,
        maximum: usize,
    },
    UnreadableConfig {
        path: PathBuf,
        detail: String,
    },
    SymlinkConfig {
        path: PathBuf,
    },
    ArtifactLockTooLarge {
        path: PathBuf,
        actual: u64,
        maximum: usize,
    },
    UnreadableArtifactLock {
        path: PathBuf,
        detail: String,
    },
    SymlinkArtifactLock {
        path: PathBuf,
    },
    DepthLimit {
        path: PathBuf,
        maximum: usize,
    },
}

impl Display for ProjectDiscoveryIssue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigTooLarge {
                path,
                actual,
                maximum,
            } => write!(
                formatter,
                "project config '{}' is {actual} bytes; maximum is {maximum} bytes",
                path.display()
            ),
            Self::UnreadableConfig { path, detail } => write!(
                formatter,
                "project config '{}' cannot be read: {detail}",
                path.display()
            ),
            Self::SymlinkConfig { path } => write!(
                formatter,
                "project config '{}' must be a regular file, not a symbolic link",
                path.display()
            ),
            Self::ArtifactLockTooLarge {
                path,
                actual,
                maximum,
            } => write!(
                formatter,
                "project artifact lock '{}' is {actual} bytes; maximum is {maximum} bytes",
                path.display()
            ),
            Self::UnreadableArtifactLock { path, detail } => write!(
                formatter,
                "project artifact lock '{}' cannot be read: {detail}",
                path.display()
            ),
            Self::SymlinkArtifactLock { path } => write!(
                formatter,
                "project artifact lock '{}' must be a regular file, not a symbolic link",
                path.display()
            ),
            Self::DepthLimit { path, maximum } => write!(
                formatter,
                "project discovery did not descend into '{}' because maximum depth is {maximum}",
                path.display()
            ),
        }
    }
}
