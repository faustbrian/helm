#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

/// Host paths for one private RabbitMQ config and definitions mount.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct StoredRabbitMqPaths {
    directory: PathBuf,
    mount_directory: PathBuf,
    config_file: PathBuf,
    definitions_file: PathBuf,
}

impl StoredRabbitMqPaths {
    pub(super) fn new(
        directory: PathBuf,
        mount_directory: PathBuf,
        config_file: PathBuf,
        definitions_file: PathBuf,
    ) -> Self {
        Self {
            directory,
            mount_directory,
            config_file,
            definitions_file,
        }
    }

    #[cfg(test)]
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    #[cfg(test)]
    pub(crate) fn mount_directory(&self) -> &Path {
        &self.mount_directory
    }

    #[cfg(test)]
    pub(crate) fn config_file(&self) -> &Path {
        &self.config_file
    }

    #[cfg(test)]
    pub(crate) fn definitions_file(&self) -> &Path {
        &self.definitions_file
    }
}
