use super::{RedisAclSnapshot, RedisPlanError, StoredRedisAclPaths};
use std::path::Path;

/// Atomically replaces the complete ACL file inside one stable mounted directory.
#[cfg(unix)]
pub(crate) fn store_redis_acl_snapshot(
    snapshot: &RedisAclSnapshot,
    directory: &Path,
) -> Result<StoredRedisAclPaths, RedisPlanError> {
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    fs::create_dir_all(directory)
        .map_err(|error| io_error("create ACL directory", directory, error))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("restrict ACL directory", directory, error))?;

    let mount_directory = directory.join("mounted");
    fs::create_dir_all(&mount_directory)
        .map_err(|error| io_error("create ACL mount directory", &mount_directory, error))?;
    fs::set_permissions(&mount_directory, fs::Permissions::from_mode(0o755))
        .map_err(|error| io_error("prepare ACL mount directory", &mount_directory, error))?;
    let _directory_lock = crate::control_plane::lock_directory(&mount_directory)
        .map_err(|error| io_error("lock ACL mount directory", &mount_directory, error))?;

    let acl_file = mount_directory.join("users.acl");
    let temporary = mount_directory.join(".users.tmp");
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(io_error("remove interrupted ACL file", &temporary, error));
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&temporary)
        .map_err(|error| io_error("create temporary ACL file", &temporary, error))?;
    file.write_all(snapshot.contents().as_bytes())
        .map_err(|error| io_error("write temporary ACL file", &temporary, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary ACL file", &temporary, error))?;
    fs::rename(&temporary, &acl_file)
        .map_err(|error| io_error("publish ACL file", &acl_file, error))?;
    fs::set_permissions(&acl_file, fs::Permissions::from_mode(0o644))
        .map_err(|error| io_error("prepare ACL file", &acl_file, error))?;
    File::open(&mount_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync ACL mount directory", &mount_directory, error))?;

    Ok(StoredRedisAclPaths::new(
        directory.to_path_buf(),
        mount_directory,
        acl_file,
    ))
}

#[cfg(unix)]
fn io_error(action: &str, path: &Path, error: std::io::Error) -> RedisPlanError {
    RedisPlanError::new(format!("failed to {action} '{}': {error}", path.display()))
}
