use super::{SqliteStateStore, StateStoreError};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

const RETAINED_STATE_BACKUPS: usize = 3;

impl SqliteStateStore {
    /// Verifies and snapshots existing state before migration or daemon mutation.
    pub(crate) fn open_with_backups(
        database_path: &Path,
        backup_directory: &Path,
        created_at_unix_seconds: i64,
    ) -> Result<Self, StateStoreError> {
        if created_at_unix_seconds < 0 {
            return Err(StateStoreError::CorruptState {
                detail: "state backup time must not be negative".to_owned(),
            });
        }
        if source_needs_backup(database_path)? {
            create_verified_backup(database_path, backup_directory, created_at_unix_seconds)?;
        }

        Self::open(database_path)
    }
}

fn source_needs_backup(path: &Path) -> Result<bool, StateStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Ok(metadata.len() > 0)
        }
        Ok(_) => Err(StateStoreError::CorruptState {
            detail: format!(
                "state database path '{}' must be a real file",
                path.display()
            ),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(backup_io("inspect source", path, source)),
    }
}

fn create_verified_backup(
    database_path: &Path,
    backup_directory: &Path,
    created_at_unix_seconds: i64,
) -> Result<(), StateStoreError> {
    prepare_backup_directory(backup_directory)?;
    let connection = Connection::open(database_path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    verify_connection(&connection, "before backup")?;

    let destination = backup_directory.join(format!("state-{created_at_unix_seconds:020}.sqlite3"));
    if destination.exists() {
        verify_backup_file(&destination)?;
        prune_old_backups(backup_directory)?;

        return Ok(());
    }
    let pending = pending_path(backup_directory, created_at_unix_seconds);
    if pending.exists() {
        fs::remove_file(&pending)
            .map_err(|source| backup_io("remove incomplete", &pending, source))?;
    }
    let pending_text = pending
        .to_str()
        .ok_or_else(|| StateStoreError::NonUtf8Path {
            path: pending.clone(),
        })?;
    connection
        .execute("VACUUM main INTO ?1", [pending_text])
        .map_err(|error| StateStoreError::CorruptState {
            detail: format!("consistent state backup failed: {error}"),
        })?;
    verify_backup_file(&pending)?;
    protect_backup_file(&pending)?;
    fs::File::open(&pending)
        .and_then(|file| file.sync_all())
        .map_err(|source| backup_io("sync pending", &pending, source))?;
    fs::rename(&pending, &destination)
        .map_err(|source| backup_io("publish", &destination, source))?;
    sync_directory(backup_directory)?;
    prune_old_backups(backup_directory)?;

    Ok(())
}

fn verify_connection(connection: &Connection, context: &str) -> Result<(), StateStoreError> {
    let integrity = connection
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| StateStoreError::CorruptState {
            detail: format!("integrity check failed {context}: {error}"),
        })?;
    if integrity != "ok" {
        return Err(StateStoreError::CorruptState {
            detail: format!("integrity check failed {context}: {integrity}"),
        });
    }

    Ok(())
}

fn verify_backup_file(path: &Path) -> Result<(), StateStoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| backup_io("inspect recovery point", path, source))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(StateStoreError::CorruptState {
            detail: format!(
                "state backup recovery point '{}' must be a real file",
                path.display()
            ),
        });
    }
    let connection = Connection::open(path)?;
    verify_connection(&connection, "in recovery point")
}

fn prepare_backup_directory(path: &Path) -> Result<(), StateStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(StateStoreError::CorruptState {
                detail: format!(
                    "state backup path '{}' must be a real directory",
                    path.display()
                ),
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(path)
            .map_err(|source| backup_io("create directory", path, source))?,
        Err(source) => return Err(backup_io("inspect directory", path, source)),
    }
    protect_backup_directory(path)
}

#[cfg(unix)]
fn protect_backup_directory(path: &Path) -> Result<(), StateStoreError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|source| backup_io("protect directory", path, source))
}

#[cfg(unix)]
fn protect_backup_file(path: &Path) -> Result<(), StateStoreError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|source| backup_io("protect file", path, source))
}

fn prune_old_backups(path: &Path) -> Result<(), StateStoreError> {
    let mut backups = Vec::new();
    for entry in fs::read_dir(path).map_err(|source| backup_io("read directory", path, source))? {
        let entry = entry.map_err(|source| backup_io("read directory entry", path, source))?;
        let entry = entry.path();
        if entry
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("state-") && name.ends_with(".sqlite3"))
        {
            backups.push(entry);
        }
    }
    backups.sort();
    let remove_count = backups.len().saturating_sub(RETAINED_STATE_BACKUPS);
    for backup in backups.into_iter().take(remove_count) {
        fs::remove_file(&backup)
            .map_err(|source| backup_io("prune recovery point", &backup, source))?;
    }
    sync_directory(path)
}

fn sync_directory(path: &Path) -> Result<(), StateStoreError> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| backup_io("sync directory", path, source))
}

fn pending_path(directory: &Path, created_at_unix_seconds: i64) -> PathBuf {
    directory.join(format!(
        ".pending-{created_at_unix_seconds:020}-{}.sqlite3",
        std::process::id()
    ))
}

fn backup_io(action: &'static str, path: &Path, source: std::io::Error) -> StateStoreError {
    StateStoreError::StateBackupIo {
        action,
        path: path.to_path_buf(),
        source,
    }
}
