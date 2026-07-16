use super::UnixDaemonRuntimeError;
use std::path::PathBuf;

/// Resolves the single predictable per-user v8 daemon state directory.
pub(crate) fn default_unix_daemon_runtime_directory() -> Result<PathBuf, UnixDaemonRuntimeError> {
    let home = std::env::var_os("HOME").ok_or_else(|| UnixDaemonRuntimeError::InvalidOptions {
        detail: "HOME is not set".to_owned(),
    })?;
    Ok(PathBuf::from(home).join(".stackctl"))
}

#[cfg(test)]
mod tests {
    use super::default_unix_daemon_runtime_directory;
    use std::path::PathBuf;

    #[test]
    fn daemon_state_has_one_predictable_home_directory() {
        let home = PathBuf::from(std::env::var_os("HOME").expect("test HOME"));

        assert_eq!(
            default_unix_daemon_runtime_directory().expect("runtime directory"),
            home.join(".stackctl")
        );
    }
}
