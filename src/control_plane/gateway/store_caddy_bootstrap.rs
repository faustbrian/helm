use super::{CaddyGatewayDocument, GatewayError, StoredGatewayBootstrapPaths};
use std::path::Path;

/// Atomically persists one immutable bootstrap document and private runtime.
#[cfg(unix)]
pub(crate) fn store_caddy_bootstrap(
    document: &CaddyGatewayDocument,
    config_path: &Path,
    runtime_directory: &Path,
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

    for directory in [config_directory, runtime_directory] {
        fs::create_dir_all(directory)
            .map_err(|error| io_error("create gateway directory", directory, error))?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error("restrict gateway directory", directory, error))?;
    }

    if config_path.exists() {
        verify_existing(config_path, document.bytes())?;
        fs::set_permissions(config_path, fs::Permissions::from_mode(0o600))
            .map_err(|error| io_error("restrict gateway bootstrap", config_path, error))?;

        return Ok(StoredGatewayBootstrapPaths::new(
            config_path.to_path_buf(),
            runtime_directory.to_path_buf(),
        ));
    }

    let temporary_path = config_directory.join(format!(
        ".config-{}-{}.tmp",
        std::process::id(),
        document.revision().replace(':', "-")
    ));
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

    Ok(StoredGatewayBootstrapPaths::new(
        config_path.to_path_buf(),
        runtime_directory.to_path_buf(),
    ))
}

#[cfg(not(unix))]
pub(crate) fn store_caddy_bootstrap(
    _document: &CaddyGatewayDocument,
    config_path: &Path,
    _runtime_directory: &Path,
) -> Result<StoredGatewayBootstrapPaths, GatewayError> {
    Err(GatewayError::Provider {
        detail: format!(
            "secure gateway bootstrap persistence is not implemented for '{}'",
            config_path.display()
        ),
    })
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
