use super::UnixDaemonRuntimeError;
use std::path::PathBuf;

/// Resolves the OS-conventional per-user v8 daemon state directory.
pub(crate) fn default_unix_daemon_runtime_directory() -> Result<PathBuf, UnixDaemonRuntimeError> {
    let home = std::env::var_os("HOME").ok_or_else(|| UnixDaemonRuntimeError::InvalidOptions {
        detail: "HOME is not set".to_owned(),
    })?;
    let home = PathBuf::from(home);

    if cfg!(target_os = "macos") {
        return Ok(home.join("Library/Application Support/stackctl"));
    }
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("stackctl"));
    }

    Ok(home.join(".local/state/stackctl"))
}
