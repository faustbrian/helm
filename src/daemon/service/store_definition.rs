use anyhow::{Context, Result, anyhow};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Atomically publishes a user-service definition without following its path.
pub(super) fn store_definition(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("service definition '{}' has no parent", path.display()))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("service definition '{}' has no UTF-8 name", path.display()))?;
    let (mut file, staging) = create_staging_file(parent, name)?;

    let mut publish = || -> Result<()> {
        file.write_all(contents.as_bytes())
            .with_context(|| format!("failed to write {}", staging.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", staging.display()))?;
        fs::rename(&staging, path)
            .with_context(|| format!("failed to publish {}", path.display()))?;

        #[cfg(unix)]
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| format!("failed to sync {}", parent.display()))?;

        Ok(())
    };

    if let Err(error) = publish() {
        drop(fs::remove_file(&staging));
        return Err(error);
    }

    Ok(())
}

fn create_staging_file(parent: &Path, name: &str) -> Result<(File, PathBuf)> {
    for attempt in 0..16 {
        let path = parent.join(format!(
            ".{name}.{}.{attempt}.stackctl.tmp",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to create {}", path.display()));
            }
        }
    }

    Err(anyhow!(
        "failed to allocate a service definition staging file under '{}'",
        parent.display()
    ))
}
