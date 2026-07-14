use super::{
    CertificateRotationLock, CertificateStoreLock, LocalCertificateBundle, LocalCertificateError,
    StoredCertificatePaths, certificate_rotation_lock::CERTIFICATE_ROTATION_LOCK_FILE,
    certificate_store_lock::CERTIFICATE_STORE_LOCK_FILE,
};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

const ACTIVE_GENERATION_FILE: &str = "current";
const PENDING_ACTIVE_GENERATION_FILE: &str = ".current.tmp";

/// Persists immutable certificate revisions without exposing partial bundles.
pub(crate) struct FilesystemCertificateStore {
    root: PathBuf,
}

impl FilesystemCertificateStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Serializes load, persist, trust, and activation across daemon and CLI.
    #[cfg(unix)]
    pub(crate) fn lock(&self) -> Result<CertificateStoreLock, LocalCertificateError> {
        CertificateStoreLock::acquire(&self.root)
    }

    /// Allows ordinary trust operations while excluding CA rotation.
    #[cfg(unix)]
    pub(crate) fn lock_trust_operation(
        &self,
    ) -> Result<CertificateRotationLock, LocalCertificateError> {
        CertificateRotationLock::acquire_shared(&self.root)
    }

    /// Excludes other trust operations for the full CA rotation transaction.
    #[cfg(unix)]
    pub(crate) fn lock_rotation(&self) -> Result<CertificateRotationLock, LocalCertificateError> {
        CertificateRotationLock::acquire_exclusive(&self.root)
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
        let mut active_generation = None;
        let entries = fs::read_dir(&self.root)
            .map_err(|error| io_error("read certificate root", &self.root, error))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| io_error("read certificate root entry", &self.root, error))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| io_error("inspect certificate root entry", &path, error))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == ACTIVE_GENERATION_FILE && file_type.is_file() {
                active_generation = Some(read_active_generation(&path)?);
                continue;
            }
            if name == PENDING_ACTIVE_GENERATION_FILE && file_type.is_file() {
                continue;
            }
            if name == CERTIFICATE_STORE_LOCK_FILE && file_type.is_file() {
                continue;
            }
            if name == CERTIFICATE_ROTATION_LOCK_FILE && file_type.is_file() {
                continue;
            }
            if !file_type.is_dir() || !is_bundle_directory(&path) {
                return Err(LocalCertificateError::new(format!(
                    "certificate root '{}' contains unexpected entry '{}'",
                    self.root.display(),
                    entry.file_name().to_string_lossy()
                )));
            }
            directories.push(path);
        }
        directories.sort();

        if let Some(active_generation) = active_generation {
            let directory = self.root.join(active_generation);
            if !directories.contains(&directory) {
                return Err(LocalCertificateError::new(format!(
                    "certificate root '{}' selects a missing active generation",
                    self.root.display()
                )));
            }

            return self.load_directory(&directory).map(Some);
        }

        Ok(None)
    }

    #[cfg(unix)]
    pub(crate) fn persist(
        &self,
        bundle: &LocalCertificateBundle,
    ) -> Result<StoredCertificatePaths, LocalCertificateError> {
        let paths = self.persist_inactive(bundle)?;
        self.activate(&paths)?;

        Ok(paths)
    }

    /// Persists one immutable generation without changing the active bundle.
    #[cfg(unix)]
    pub(crate) fn persist_inactive(
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

    /// Atomically selects one verified immutable generation as current.
    #[cfg(unix)]
    pub(crate) fn activate(
        &self,
        paths: &StoredCertificatePaths,
    ) -> Result<(), LocalCertificateError> {
        use std::os::unix::fs::OpenOptionsExt;

        self.load_directory(paths.directory())?;
        let generation = paths
            .directory()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| LocalCertificateError::new("certificate generation is not Unicode"))?;
        let pending = self.root.join(PENDING_ACTIVE_GENERATION_FILE);
        match fs::remove_file(&pending) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(io_error(
                    "remove stale active certificate pointer",
                    &pending,
                    error,
                ));
            }
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending)
            .map_err(|error| io_error("create active certificate pointer", &pending, error))?;
        file.write_all(format!("{generation}\n").as_bytes())
            .map_err(|error| io_error("write active certificate pointer", &pending, error))?;
        file.sync_all()
            .map_err(|error| io_error("sync active certificate pointer", &pending, error))?;
        let active = self.root.join(ACTIVE_GENERATION_FILE);
        fs::rename(&pending, &active)
            .map_err(|error| io_error("publish active certificate pointer", &active, error))?;
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error("sync certificate root", &self.root, error))
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

fn read_active_generation(path: &Path) -> Result<String, LocalCertificateError> {
    let generation = fs::read_to_string(path)
        .map_err(|error| io_error("read active certificate pointer", path, error))?;
    let generation = generation.trim();
    if generation.len() != "bundle-".len() + 64
        || !generation.starts_with("bundle-")
        || !generation["bundle-".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(LocalCertificateError::new(format!(
            "active certificate pointer '{}' is malformed",
            path.display()
        )));
    }

    Ok(generation.to_owned())
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
