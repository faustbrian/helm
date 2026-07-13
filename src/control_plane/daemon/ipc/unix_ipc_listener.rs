use super::IpcError;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

/// A user-only Unix socket owned for the daemon process lifetime.
#[derive(Debug)]
pub(crate) struct UnixIpcListener {
    path: PathBuf,
    _listener: UnixListener,
}

impl UnixIpcListener {
    /// Binds a new endpoint without deleting or replacing an existing path.
    pub(crate) fn bind(path: &Path) -> Result<Self, IpcError> {
        let listener = UnixListener::bind(path).map_err(|source| IpcError::EndpointIo {
            path: path.to_path_buf(),
            source,
        })?;

        if let Err(source) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        {
            drop(listener);
            let _cleanup_result = std::fs::remove_file(path);

            return Err(IpcError::EndpointIo {
                path: path.to_path_buf(),
                source,
            });
        }

        Ok(Self {
            path: path.to_path_buf(),
            _listener: listener,
        })
    }
}

impl Drop for UnixIpcListener {
    fn drop(&mut self) {
        let _cleanup_result = std::fs::remove_file(&self.path);
    }
}
