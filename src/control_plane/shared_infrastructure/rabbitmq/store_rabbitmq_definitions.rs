use super::{RabbitMqDefinitions, RabbitMqPlanError, StoredRabbitMqPaths};
use std::path::Path;

const RABBITMQ_CONFIG: &[u8] = b"definitions.import_backend = local_filesystem\n\
definitions.local.path = /etc/stackctl/rabbitmq/definitions.json\n\
definitions.skip_if_unchanged = true\n";

/// Persists immutable broker config and atomically replaceable hash-only definitions.
#[cfg(unix)]
pub(crate) fn store_rabbitmq_definitions(
    definitions: &RabbitMqDefinitions,
    directory: &Path,
) -> Result<StoredRabbitMqPaths, RabbitMqPlanError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(directory)
        .map_err(|error| io_error("create definitions directory", directory, error))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("restrict definitions directory", directory, error))?;
    let mount_directory = directory.join("mounted");
    fs::create_dir_all(&mount_directory)
        .map_err(|error| io_error("create definitions mount", &mount_directory, error))?;
    fs::set_permissions(&mount_directory, fs::Permissions::from_mode(0o755))
        .map_err(|error| io_error("prepare definitions mount", &mount_directory, error))?;

    let config_file = mount_directory.join("rabbitmq.conf");
    store_immutable_file(&config_file, RABBITMQ_CONFIG)?;
    let definitions_file = mount_directory.join("definitions.json");
    replace_file(&definitions_file, definitions.contents())?;

    Ok(StoredRabbitMqPaths::new(
        directory.to_path_buf(),
        mount_directory,
        config_file,
        definitions_file,
    ))
}

#[cfg(unix)]
fn store_immutable_file(path: &Path, contents: &[u8]) -> Result<(), RabbitMqPlanError> {
    if path.exists() {
        let found =
            std::fs::read(path).map_err(|error| io_error("read broker config", path, error))?;
        if found != contents {
            return Err(RabbitMqPlanError::new(format!(
                "existing RabbitMQ config '{}' does not match the managed config",
                path.display()
            )));
        }
        return Ok(());
    }

    replace_file(path, contents)
}

#[cfg(unix)]
fn replace_file(path: &Path, contents: &[u8]) -> Result<(), RabbitMqPlanError> {
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let directory = path.parent().ok_or_else(|| {
        RabbitMqPlanError::new(format!("RabbitMQ path '{}' has no parent", path.display()))
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            RabbitMqPlanError::new(format!(
                "RabbitMQ path '{}' is not valid UTF-8",
                path.display()
            ))
        })?;
    let temporary = directory.join(format!(".{file_name}-{}.tmp", std::process::id()));
    if temporary.exists() {
        return Err(RabbitMqPlanError::new(format!(
            "temporary RabbitMQ file '{}' already exists",
            temporary.display()
        )));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&temporary)
        .map_err(|error| io_error("create temporary RabbitMQ file", &temporary, error))?;
    file.write_all(contents)
        .map_err(|error| io_error("write temporary RabbitMQ file", &temporary, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary RabbitMQ file", &temporary, error))?;
    fs::rename(&temporary, path).map_err(|error| io_error("publish RabbitMQ file", path, error))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644))
        .map_err(|error| io_error("prepare RabbitMQ file", path, error))?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync RabbitMQ directory", directory, error))?;

    Ok(())
}

#[cfg(unix)]
fn io_error(action: &str, path: &Path, error: std::io::Error) -> RabbitMqPlanError {
    RabbitMqPlanError::new(format!("failed to {action} '{}': {error}", path.display()))
}
