use std::path::{Path, PathBuf};

/// Host paths for one private Mailpit SMTP authentication mount.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct StoredMailpitAuthenticationPaths {
    directory: PathBuf,
    mount_directory: PathBuf,
    password_file: PathBuf,
}

impl StoredMailpitAuthenticationPaths {
    pub(super) fn new(
        directory: PathBuf,
        mount_directory: PathBuf,
        password_file: PathBuf,
    ) -> Self {
        Self {
            directory,
            mount_directory,
            password_file,
        }
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn mount_directory(&self) -> &Path {
        &self.mount_directory
    }

    pub(crate) fn password_file(&self) -> &Path {
        &self.password_file
    }
}
