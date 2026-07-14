use super::{MailpitAuthenticationSnapshot, MailpitPlanError, StoredMailpitAuthenticationPaths};
use std::path::Path;

/// Atomically persists one complete hash-only Mailpit SMTP password file.
#[cfg(unix)]
pub(crate) fn store_mailpit_authentication(
    snapshot: &MailpitAuthenticationSnapshot,
    directory: &Path,
) -> Result<StoredMailpitAuthenticationPaths, MailpitPlanError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(directory)
        .map_err(|error| io_error("create authentication directory", directory, error))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("restrict authentication directory", directory, error))?;
    let mount_directory = directory.join("mounted");
    fs::create_dir_all(&mount_directory)
        .map_err(|error| io_error("create authentication mount", &mount_directory, error))?;
    fs::set_permissions(&mount_directory, fs::Permissions::from_mode(0o755))
        .map_err(|error| io_error("prepare authentication mount", &mount_directory, error))?;
    let _directory_lock = crate::control_plane::lock_directory(&mount_directory)
        .map_err(|error| io_error("lock authentication mount", &mount_directory, error))?;
    let password_file = mount_directory.join("smtp-passwords");
    replace_file(&password_file, snapshot.contents())?;

    Ok(StoredMailpitAuthenticationPaths::new(
        directory.to_path_buf(),
        mount_directory,
        password_file,
    ))
}

#[cfg(unix)]
fn replace_file(path: &Path, contents: &[u8]) -> Result<(), MailpitPlanError> {
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let directory = path.parent().ok_or_else(|| {
        MailpitPlanError::new(format!("Mailpit path '{}' has no parent", path.display()))
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            MailpitPlanError::new(format!(
                "Mailpit path '{}' is not valid UTF-8",
                path.display()
            ))
        })?;
    let temporary = directory.join(format!(".{file_name}.tmp"));
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(io_error(
                "remove interrupted Mailpit file",
                &temporary,
                error,
            ));
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&temporary)
        .map_err(|error| io_error("create temporary password file", &temporary, error))?;
    file.write_all(contents)
        .map_err(|error| io_error("write temporary password file", &temporary, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary password file", &temporary, error))?;
    fs::rename(&temporary, path).map_err(|error| io_error("publish password file", path, error))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644))
        .map_err(|error| io_error("prepare password file", path, error))?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync Mailpit directory", directory, error))?;

    Ok(())
}

#[cfg(unix)]
fn io_error(action: &str, path: &Path, error: std::io::Error) -> MailpitPlanError {
    MailpitPlanError::new(format!("failed to {action} '{}': {error}", path.display()))
}
