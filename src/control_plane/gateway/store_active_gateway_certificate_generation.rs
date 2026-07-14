use super::GatewayError;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub(super) const ACTIVE_GATEWAY_CERTIFICATE_GENERATION_FILE: &str = "active-certificate-generation";

/// Atomically publishes the certificate generation served by the ready gateway.
#[cfg(unix)]
pub(crate) fn store_active_gateway_certificate_generation(
    runtime_directory: &Path,
    generation: &str,
) -> Result<(), GatewayError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    validate_generation(generation)?;
    let directory = runtime_directory.join("gateway");
    fs::create_dir_all(&directory).map_err(|error| failure("create", &directory, error))?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| failure("restrict", &directory, error))?;
    let _directory_lock = crate::control_plane::lock_directory(&directory)
        .map_err(|error| failure("lock", &directory, error))?;
    let active = directory.join(ACTIVE_GATEWAY_CERTIFICATE_GENERATION_FILE);
    let pending = directory.join(format!(".{ACTIVE_GATEWAY_CERTIFICATE_GENERATION_FILE}.tmp"));
    match fs::remove_file(&pending) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(failure("remove stale", &pending, error)),
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&pending)
        .map_err(|error| failure("create", &pending, error))?;
    file.write_all(format!("{generation}\n").as_bytes())
        .map_err(|error| failure("write", &pending, error))?;
    file.sync_all()
        .map_err(|error| failure("sync", &pending, error))?;
    fs::rename(&pending, &active).map_err(|error| failure("publish", &active, error))?;
    File::open(&directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| failure("sync", &directory, error))?;

    Ok(())
}

fn validate_generation(generation: &str) -> Result<(), GatewayError> {
    if generation.len() != 64 || !generation.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(GatewayError::InvalidPlan {
            detail: "gateway certificate generation must be a 64-character SHA-256 revision"
                .to_owned(),
        });
    }

    Ok(())
}

fn failure(action: &str, path: &Path, error: std::io::Error) -> GatewayError {
    GatewayError::Reconciliation {
        detail: format!(
            "failed to {action} active gateway certificate generation '{}': {error}",
            path.display()
        ),
    }
}
