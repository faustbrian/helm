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

    let acl_file = directory.join("users.acl");
    let temporary = directory.join(format!(".users-{}.tmp", std::process::id()));
    if temporary.exists() {
        return Err(RedisPlanError::new(format!(
            "temporary Redis ACL file '{}' already exists",
            temporary.display()
        )));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| io_error("create temporary ACL file", &temporary, error))?;
    file.write_all(snapshot.contents().as_bytes())
        .map_err(|error| io_error("write temporary ACL file", &temporary, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary ACL file", &temporary, error))?;
    fs::rename(&temporary, &acl_file)
        .map_err(|error| io_error("publish ACL file", &acl_file, error))?;
    fs::set_permissions(&acl_file, fs::Permissions::from_mode(0o600))
        .map_err(|error| io_error("restrict ACL file", &acl_file, error))?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync ACL directory", directory, error))?;

    Ok(StoredRedisAclPaths::new(directory.to_path_buf(), acl_file))
}

#[cfg(not(unix))]
pub(crate) fn store_redis_acl_snapshot(
    _snapshot: &RedisAclSnapshot,
    directory: &Path,
) -> Result<StoredRedisAclPaths, RedisPlanError> {
    Err(RedisPlanError::new(format!(
        "secure Redis ACL persistence is not implemented for '{}' on this platform",
        directory.display()
    )))
}

#[cfg(unix)]
fn io_error(action: &str, path: &Path, error: std::io::Error) -> RedisPlanError {
    RedisPlanError::new(format!("failed to {action} '{}': {error}", path.display()))
}
