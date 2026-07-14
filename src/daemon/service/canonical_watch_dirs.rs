use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

pub(crate) fn canonical_watch_dirs(directories: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut canonical_directories = Vec::with_capacity(directories.len());
    let mut unique = BTreeSet::new();
    for directory in directories {
        let canonical = fs::canonicalize(directory)
            .with_context(|| format!("failed to resolve watched root '{}'", directory.display()))?;
        let metadata = fs::metadata(&canonical)
            .with_context(|| format!("failed to inspect watched root '{}'", directory.display()))?;
        if !metadata.is_dir() {
            bail!("watched root '{}' must be a directory", directory.display());
        }
        if !unique.insert(canonical.clone()) {
            bail!(
                "watched root '{}' resolves to a directory that was already configured",
                directory.display()
            );
        }
        canonical_directories.push(canonical);
    }

    Ok(canonical_directories)
}
