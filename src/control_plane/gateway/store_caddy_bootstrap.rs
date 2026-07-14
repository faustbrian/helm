use super::{CaddyGatewayDocument, GatewayError, StoredGatewayBootstrapPaths};
use std::path::Path;

/// Atomically persists one immutable bootstrap document.
#[cfg(unix)]
pub(crate) fn store_caddy_bootstrap(
    document: &CaddyGatewayDocument,
    config_path: &Path,
) -> Result<StoredGatewayBootstrapPaths, GatewayError> {
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let config_directory = config_path
        .parent()
        .ok_or_else(|| GatewayError::InvalidPlan {
            detail: format!(
                "gateway bootstrap path '{}' must have a parent directory",
                config_path.display()
            ),
        })?;

    fs::create_dir_all(config_directory)
        .map_err(|error| io_error("create gateway directory", config_directory, error))?;
    fs::set_permissions(config_directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("restrict gateway directory", config_directory, error))?;

    let temporary_path = config_directory.join(".config.tmp");
    match fs::remove_file(&temporary_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(io_error(
                "remove interrupted gateway bootstrap",
                &temporary_path,
                error,
            ));
        }
    }

    if config_path.exists() {
        verify_existing(config_path, document.bytes())?;
        fs::set_permissions(config_path, fs::Permissions::from_mode(0o600))
            .map_err(|error| io_error("restrict gateway bootstrap", config_path, error))?;
        File::open(config_directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error("sync gateway config directory", config_directory, error))?;

        return Ok(StoredGatewayBootstrapPaths::new(config_path.to_path_buf()));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary_path)
        .map_err(|error| io_error("create gateway bootstrap", &temporary_path, error))?;
    file.write_all(document.bytes())
        .map_err(|error| io_error("write gateway bootstrap", &temporary_path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync gateway bootstrap", &temporary_path, error))?;
    fs::rename(&temporary_path, config_path)
        .map_err(|error| io_error("publish gateway bootstrap", config_path, error))?;
    File::open(config_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync gateway config directory", config_directory, error))?;

    Ok(StoredGatewayBootstrapPaths::new(config_path.to_path_buf()))
}

#[cfg(unix)]
fn verify_existing(path: &Path, expected: &[u8]) -> Result<(), GatewayError> {
    let found = std::fs::read(path)
        .map_err(|error| io_error("read existing gateway bootstrap", path, error))?;

    if found != expected {
        return Err(GatewayError::Provider {
            detail: format!(
                "existing gateway bootstrap '{}' does not match revision",
                path.display()
            ),
        });
    }

    Ok(())
}

#[cfg(unix)]
fn io_error(action: &str, path: &Path, error: std::io::Error) -> GatewayError {
    GatewayError::Provider {
        detail: format!("failed to {action} '{}': {error}", path.display()),
    }
}
