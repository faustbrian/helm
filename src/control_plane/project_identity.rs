use super::{DnsLabel, IdentityError};
use std::path::Path;

/// The exact validated identity of a v8 project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectIdentity(DnsLabel);

impl ProjectIdentity {
    /// Uses the explicit name when present, otherwise the exact directory name.
    pub(crate) fn resolve(
        explicit_name: Option<&str>,
        project_directory: &Path,
    ) -> Result<Self, IdentityError> {
        let name = match explicit_name {
            Some(name) => name,
            None => project_directory
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .ok_or_else(|| IdentityError::MissingDirectoryBasename {
                    path: project_directory.to_path_buf(),
                })?,
        };

        DnsLabel::new("project", name).map(Self)
    }

    /// Returns the exact name supplied by configuration or the directory.
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
