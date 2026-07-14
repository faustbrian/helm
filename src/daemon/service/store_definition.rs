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
    let directory_lock = File::open(parent).with_context(|| {
        format!(
            "failed to open service definition directory {}",
            parent.display()
        )
    })?;
    directory_lock.lock().with_context(|| {
        format!(
            "failed to lock service definition directory {}",
            parent.display()
        )
    })?;
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
    let path = parent.join(format!(".{name}.stackctl.tmp"));
    match fs::remove_file(&path) {
        Ok(()) => File::open(parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| format!("failed to sync {}", parent.display()))?,
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to remove stale {}", path.display()));
        }
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to create {}", path.display()))?;

    Ok((file, path))
}

#[cfg(test)]
mod tests {
    use super::store_definition;

    #[test]
    fn publication_recovers_one_stable_interrupted_staging_file() {
        let root = std::env::temp_dir().join(format!(
            "stackctl-service-definition-staging-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&root).expect("create definition directory");
        let definition = root.join("dev.stackctl.daemon.plist");
        let pending = root.join(".dev.stackctl.daemon.plist.stackctl.tmp");
        std::fs::write(&pending, "interrupted").expect("write interrupted staging file");

        store_definition(&definition, "complete").expect("publish service definition");

        assert_eq!(
            std::fs::read_to_string(&definition).expect("read definition"),
            "complete"
        );
        assert!(!pending.exists());

        std::fs::remove_dir_all(root).expect("remove definition fixture");
    }
}
