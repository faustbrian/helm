use super::store_definition::store_definition;
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::Path;

/// Exact host-file state needed to reverse one service installation attempt.
pub(super) struct ServiceInstallSnapshot {
    previous_contents: Option<String>,
    was_running: bool,
}

impl ServiceInstallSnapshot {
    pub(super) fn capture(path: &Path, was_running: bool) -> Result<Self> {
        let previous_contents = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Some(
                fs::read_to_string(path)
                    .with_context(|| format!("failed to snapshot {}", path.display()))?,
            ),
            Ok(_) => None,
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
            }
        };

        Ok(Self {
            previous_contents,
            was_running,
        })
    }

    pub(super) const fn was_running(&self) -> bool {
        self.was_running
    }

    pub(super) fn restore(self, path: &Path) -> Result<()> {
        if let Some(contents) = self.previous_contents {
            return store_definition(path, &contents);
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| format!("failed to remove {}", path.display()));
            }
        }
        if let Some(parent) = path.parent() {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .with_context(|| format!("failed to sync {}", parent.display()))?;
        }

        Ok(())
    }
}
