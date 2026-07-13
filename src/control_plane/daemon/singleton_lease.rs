use super::SingletonLeaseError;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

/// A held OS lock proving this process is the per-user v8 daemon.
#[derive(Debug)]
pub(crate) struct SingletonLease {
    _file: File,
}

impl SingletonLease {
    /// Acquires exclusive ownership without consulting stale PID contents.
    pub(crate) fn acquire(lock_path: &Path) -> Result<Self, SingletonLeaseError> {
        let mut file = open_lock_file(lock_path)?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(SingletonLeaseError::AlreadyRunning {
                    path: lock_path.to_path_buf(),
                });
            }
            Err(TryLockError::Error(source)) => {
                return Err(SingletonLeaseError::Io {
                    path: lock_path.to_path_buf(),
                    source,
                });
            }
        }

        restrict_permissions(&file, lock_path)?;
        file.set_len(0)
            .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
            .and_then(|()| file.write_all(std::process::id().to_string().as_bytes()))
            .and_then(|()| file.sync_data())
            .map_err(|source| SingletonLeaseError::Io {
                path: lock_path.to_path_buf(),
                source,
            })?;

        Ok(Self { _file: file })
    }
}

fn open_lock_file(lock_path: &Path) -> Result<File, SingletonLeaseError> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    options
        .open(lock_path)
        .map_err(|source| SingletonLeaseError::Io {
            path: lock_path.to_path_buf(),
            source,
        })
}

#[cfg(unix)]
fn restrict_permissions(file: &File, lock_path: &Path) -> Result<(), SingletonLeaseError> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|source| SingletonLeaseError::Io {
            path: lock_path.to_path_buf(),
            source,
        })
}

#[cfg(not(unix))]
fn restrict_permissions(_file: &File, _lock_path: &Path) -> Result<(), SingletonLeaseError> {
    Ok(())
}
