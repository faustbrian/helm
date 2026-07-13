use std::path::PathBuf;
use std::time::Duration;

/// User-selected roots and runtime placement for Unix daemon watch mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnixDaemonWatchOptions {
    pub(crate) runtime_directory: PathBuf,
    pub(crate) watched_roots: Vec<PathBuf>,
    pub(crate) once: bool,
    pub(crate) periodic_rescan: Duration,
}
