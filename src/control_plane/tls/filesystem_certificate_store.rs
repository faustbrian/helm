use super::{LocalCertificateBundle, LocalCertificateError, StoredCertificatePaths};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Persists immutable certificate revisions without exposing partial bundles.
pub(crate) struct FilesystemCertificateStore {
    root: PathBuf,
}

impl FilesystemCertificateStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
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

        let writes = [
            ("ca.crt", bundle.ca_certificate_pem()),
            ("ca.key", bundle.ca_private_key_pem()),
            ("wildcard.crt", bundle.leaf_certificate_pem()),
            ("wildcard.key", bundle.leaf_private_key_pem()),
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
}

fn bundle_revision(bundle: &LocalCertificateBundle) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bundle.ca_certificate_pem().as_bytes());
    hasher.update(bundle.leaf_certificate_pem().as_bytes());

    hex::encode(hasher.finalize())
}

#[cfg(unix)]
fn verify_existing_bundle(
    directory: &Path,
    bundle: &LocalCertificateBundle,
) -> Result<StoredCertificatePaths, LocalCertificateError> {
    let expected = [
        ("ca.crt", bundle.ca_certificate_pem()),
        ("ca.key", bundle.ca_private_key_pem()),
        ("wildcard.crt", bundle.leaf_certificate_pem()),
        ("wildcard.key", bundle.leaf_private_key_pem()),
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

fn io_error(action: &str, path: &Path, error: std::io::Error) -> LocalCertificateError {
    LocalCertificateError::new(format!("failed to {action} '{}': {error}", path.display()))
}
