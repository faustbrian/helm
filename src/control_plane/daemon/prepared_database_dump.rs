use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const MAX_UNCOMPRESSED_DUMP_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// A directly readable SQL dump, optionally staged from one exact ZIP entry.
pub(crate) struct PreparedDatabaseDump {
    path: PathBuf,
    temporary: bool,
}

impl PreparedDatabaseDump {
    pub(crate) fn materialize(
        source: &Path,
        archive_entry: Option<&str>,
        staging_root: &Path,
        operation_id: &str,
    ) -> Result<Self, String> {
        let metadata = std::fs::symlink_metadata(source)
            .map_err(|error| format!("database dump source cannot be inspected: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
            return Err("database dump source must remain a non-empty regular file".to_owned());
        }
        let Some(archive_entry) = archive_entry else {
            return Ok(Self {
                path: source.to_path_buf(),
                temporary: false,
            });
        };
        let directory = staging_root.join("database-dump-imports");
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("database dump staging directory failed: {error}"))?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("database dump staging permissions failed: {error}"))?;
        let name = format!("{}.sql", hex::encode(Sha256::digest(operation_id)));
        let path = directory.join(name);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "stale database dump staging file cannot be removed: {error}"
                ));
            }
        }
        let archive_file = std::fs::File::open(source)
            .map_err(|error| format!("database dump archive open failed: {error}"))?;
        let mut archive = zip::ZipArchive::new(archive_file)
            .map_err(|error| format!("database dump ZIP is invalid: {error}"))?;
        let mut entry = archive.by_name(archive_entry).map_err(|error| {
            format!("database dump ZIP entry '{archive_entry}' is unavailable: {error}")
        })?;
        if !entry.is_file() || entry.size() == 0 || entry.size() > MAX_UNCOMPRESSED_DUMP_BYTES {
            return Err(format!(
                "database dump ZIP entry '{archive_entry}' must be a non-empty file no larger than 10 GiB"
            ));
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| format!("database dump staging file failed: {error}"))?;
        if let Err(error) = std::io::copy(&mut entry, &mut output) {
            drop(std::fs::remove_file(&path));

            return Err(format!("database dump ZIP extraction failed: {error}"));
        }

        Ok(Self {
            path,
            temporary: true,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PreparedDatabaseDump {
    fn drop(&mut self) {
        if self.temporary {
            drop(std::fs::remove_file(&self.path));
        }
    }
}
