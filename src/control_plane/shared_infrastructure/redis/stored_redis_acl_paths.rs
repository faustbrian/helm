use std::path::{Path, PathBuf};

/// Host paths for a directory-mounted Redis-compatible ACL snapshot.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct StoredRedisAclPaths {
    directory: PathBuf,
    mount_directory: PathBuf,
    acl_file: PathBuf,
}

impl StoredRedisAclPaths {
    pub(super) fn new(directory: PathBuf, mount_directory: PathBuf, acl_file: PathBuf) -> Self {
        Self {
            directory,
            mount_directory,
            acl_file,
        }
    }

    pub(crate) fn mount_directory(&self) -> &Path {
        &self.mount_directory
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn acl_file(&self) -> &Path {
        &self.acl_file
    }
}
