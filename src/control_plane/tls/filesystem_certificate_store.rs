use super::{LocalCertificateBundle, LocalCertificateError, StoredCertificatePaths};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

/// Persists immutable certificate revisions without exposing partial bundles.
pub(crate) struct FilesystemCertificateStore {
    root: PathBuf,
}

impl FilesystemCertificateStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Recovers the newest fully verified immutable certificate generation.
    pub(crate) fn load_current(
        &self,
    ) -> Result<Option<(LocalCertificateBundle, StoredCertificatePaths)>, LocalCertificateError>
    {
        if !self.root.exists() {
            return Ok(None);
        }
        let metadata = fs::symlink_metadata(&self.root)
            .map_err(|error| io_error("inspect certificate root", &self.root, error))?;
        if !metadata.file_type().is_dir() {
            return Err(LocalCertificateError::new(format!(
                "certificate root '{}' must be a directory",
                self.root.display()
            )));
        }

        let mut directories = Vec::new();
        let entries = fs::read_dir(&self.root)
            .map_err(|error| io_error("read certificate root", &self.root, error))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| io_error("read certificate root entry", &self.root, error))?;
            let path = entry.path();
            if !entry
                .file_type()
                .map_err(|error| io_error("inspect certificate root entry", &path, error))?
                .is_dir()
                || !is_bundle_directory(&path)
            {
                return Err(LocalCertificateError::new(format!(
                    "certificate root '{}' contains unexpected entry '{}'",
                    self.root.display(),
                    entry.file_name().to_string_lossy()
                )));
            }
            directories.push(path);
        }
        directories.sort();

        let mut current: Option<(LocalCertificateBundle, StoredCertificatePaths)> = None;
        for directory in directories {
            let candidate = self.load_directory(&directory)?;
            let Some((bundle, _paths)) = &current else {
                current = Some(candidate);
                continue;
            };
            if candidate.0.leaf_renew_after() > bundle.leaf_renew_after() {
                current = Some(candidate);
                continue;
            }
            if candidate.0.leaf_renew_after() == bundle.leaf_renew_after() && candidate.0 != *bundle
            {
                return Err(LocalCertificateError::new(format!(
                    "certificate root '{}' contains ambiguous bundles with renewal deadline {}",
                    self.root.display(),
                    bundle.leaf_renew_after()
                )));
            }
        }

        Ok(current)
    }

    #[cfg(unix)]
    pub(crate) fn persist(
        &self,
        bundle: &LocalCertificateBundle,
    ) -> Result<StoredCertificatePaths, LocalCertificateError> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

        fs::create_dir_all(&self.root)
            .map_err(|error| io_error("create certificate root", &self.root, error))?;
        fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error("restrict certificate root", &self.root, error))?;

        let revision = bundle_revision(bundle);
        let final_directory = self.root.join(format!("bundle-{revision}"));

        if final_directory.exists() {
            return verify_existing_bundle(&final_directory, bundle);
        }

        let staging_directory = self
            .root
            .join(format!(".bundle-{revision}-{}.tmp", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&staging_directory)
            .map_err(|error| {
                io_error(
                    "create certificate staging directory",
                    &staging_directory,
                    error,
                )
            })?;

        let renew_after = format!("{}\n", bundle.leaf_renew_after().unix_timestamp());
        let writes = [
            ("ca.crt", bundle.ca_certificate_pem()),
            ("ca.key", bundle.ca_private_key_pem()),
            ("wildcard.crt", bundle.leaf_certificate_pem()),
            ("wildcard.key", bundle.leaf_private_key_pem()),
            ("renew-after", renew_after.as_str()),
        ];

        for (name, contents) in writes {
            let path = staging_directory.join(name);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .map_err(|error| io_error("create certificate file", &path, error))?;
            file.write_all(contents.as_bytes())
                .map_err(|error| io_error("write certificate file", &path, error))?;
            file.sync_all()
                .map_err(|error| io_error("sync certificate file", &path, error))?;
        }

        File::open(&staging_directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| {
                io_error(
                    "sync certificate staging directory",
                    &staging_directory,
                    error,
                )
            })?;
        fs::rename(&staging_directory, &final_directory)
            .map_err(|error| io_error("publish certificate bundle", &final_directory, error))?;
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error("sync certificate root", &self.root, error))?;

        Ok(StoredCertificatePaths::new(final_directory))
    }

    #[cfg(not(unix))]
    pub(crate) fn persist(
        &self,
        _bundle: &LocalCertificateBundle,
    ) -> Result<StoredCertificatePaths, LocalCertificateError> {
        Err(LocalCertificateError::new(format!(
            "secure certificate persistence is not implemented for '{}'",
            self.root.display()
        )))
    }

    pub(crate) fn load_directory(
        &self,
        directory: &Path,
    ) -> Result<(LocalCertificateBundle, StoredCertificatePaths), LocalCertificateError> {
        if directory.parent() != Some(self.root.as_path()) || !is_bundle_directory(directory) {
            return Err(LocalCertificateError::new(format!(
                "certificate bundle directory '{}' is not an immutable revision under '{}'",
                directory.display(),
                self.root.display()
            )));
        }

        let paths = StoredCertificatePaths::new(directory.to_path_buf());
        let bundle = self.load(&paths)?;
        let expected_directory = self
            .root
            .join(format!("bundle-{}", bundle_revision(&bundle)));
        if directory != expected_directory {
            return Err(LocalCertificateError::new(format!(
                "certificate bundle directory '{}' does not match its contents",
                directory.display()
            )));
        }

        Ok((bundle, paths))
    }

    fn load(
        &self,
        paths: &StoredCertificatePaths,
    ) -> Result<LocalCertificateBundle, LocalCertificateError> {
        let ca_certificate = read_certificate_file(&paths.ca_certificate())?;
        let ca_private_key = read_certificate_file(&paths.ca_private_key())?;
        let leaf_certificate = read_certificate_file(&paths.leaf_certificate())?;
        let leaf_private_key = read_certificate_file(&paths.leaf_private_key())?;
        let renew_after_path = paths.renew_after();
        let renew_after = read_certificate_file(&renew_after_path)?
            .trim()
            .parse::<i64>()
            .map_err(|error| {
                LocalCertificateError::new(format!(
                    "certificate renewal deadline is not a valid Unix timestamp in '{}': {error}",
                    renew_after_path.display()
                ))
            })?;
        let renew_after = OffsetDateTime::from_unix_timestamp(renew_after).map_err(|error| {
            LocalCertificateError::new(format!(
                "certificate renewal deadline is outside the supported range in '{}': {error}",
                renew_after_path.display()
            ))
        })?;

        Ok(LocalCertificateBundle::new(
            ca_certificate,
            ca_private_key,
            leaf_certificate,
            leaf_private_key,
            renew_after,
        ))
    }
}

fn is_bundle_directory(directory: &Path) -> bool {
    let Some(name) = directory.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(revision) = name.strip_prefix("bundle-") else {
        return false;
    };

    revision.len() == 64 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn bundle_revision(bundle: &LocalCertificateBundle) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bundle.ca_certificate_pem().as_bytes());
    hasher.update(bundle.ca_private_key_pem().as_bytes());
    hasher.update(bundle.leaf_certificate_pem().as_bytes());
    hasher.update(bundle.leaf_private_key_pem().as_bytes());
    hasher.update(bundle.leaf_renew_after().unix_timestamp().to_be_bytes());

    hex::encode(hasher.finalize())
}

#[cfg(unix)]
fn verify_existing_bundle(
    directory: &Path,
    bundle: &LocalCertificateBundle,
) -> Result<StoredCertificatePaths, LocalCertificateError> {
    let renew_after = format!("{}\n", bundle.leaf_renew_after().unix_timestamp());
    let expected = [
        ("ca.crt", bundle.ca_certificate_pem()),
        ("ca.key", bundle.ca_private_key_pem()),
        ("wildcard.crt", bundle.leaf_certificate_pem()),
        ("wildcard.key", bundle.leaf_private_key_pem()),
        ("renew-after", renew_after.as_str()),
    ];

    for (name, contents) in expected {
        let path = directory.join(name);
        let found = fs::read_to_string(&path)
            .map_err(|error| io_error("read existing certificate file", &path, error))?;

        if found != contents {
            return Err(LocalCertificateError::new(format!(
                "existing certificate bundle '{}' does not match its revision",
                directory.display()
            )));
        }
    }

    Ok(StoredCertificatePaths::new(directory.to_path_buf()))
}

fn read_certificate_file(path: &Path) -> Result<String, LocalCertificateError> {
    fs::read_to_string(path).map_err(|error| io_error("read certificate bundle file", path, error))
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> LocalCertificateError {
    LocalCertificateError::new(format!("failed to {action} '{}': {error}", path.display()))
}
