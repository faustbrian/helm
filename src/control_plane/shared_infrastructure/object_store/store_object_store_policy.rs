use super::{ObjectStorePlanError, ObjectStoreProjectDefinition};
use std::path::{Path, PathBuf};

/// Atomically persists one bucket-scoped policy for the mounted MinIO client.
#[cfg(unix)]
pub(crate) fn store_object_store_policy(
    definition: &ObjectStoreProjectDefinition,
    directory: &Path,
) -> Result<PathBuf, ObjectStorePlanError> {
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    fs::create_dir_all(directory)
        .map_err(|error| io_error("create policy directory", directory, error))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("restrict policy directory", directory, error))?;
    let _directory_lock = crate::control_plane::lock_directory(directory)
        .map_err(|error| io_error("lock policy directory", directory, error))?;
    let path = directory.join(format!("{}.json", definition.policy_name()));
    let temporary = directory.join(format!(".{}.json.tmp", definition.policy_name()));
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(io_error(
                "remove interrupted object-store policy",
                &temporary,
                error,
            ));
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| io_error("create temporary policy", &temporary, error))?;
    file.write_all(definition.policy_json().as_bytes())
        .map_err(|error| io_error("write temporary policy", &temporary, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary policy", &temporary, error))?;
    fs::rename(&temporary, &path)
        .map_err(|error| io_error("publish object-store policy", &path, error))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .map_err(|error| io_error("restrict object-store policy", &path, error))?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync policy directory", directory, error))?;

    Ok(path)
}

#[cfg(unix)]
fn io_error(action: &str, path: &Path, error: std::io::Error) -> ObjectStorePlanError {
    ObjectStorePlanError::new(format!("failed to {action} '{}': {error}", path.display()))
}
