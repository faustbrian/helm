use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn canonical_watched_roots(roots: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut canonical_roots = Vec::with_capacity(roots.len());
    let mut unique = BTreeSet::new();
    for root in roots {
        let canonical = fs::canonicalize(root)
            .with_context(|| format!("failed to resolve watched root '{}'", root.display()))?;
        require_directory(root, &canonical)?;
        if !unique.insert(canonical.clone()) {
            bail!(
                "watched root '{}' resolves to a directory that was already configured",
                root.display()
            );
        }
        canonical_roots.push(canonical);
    }

    Ok(canonical_roots)
}

fn require_directory(original: &Path, canonical: &Path) -> Result<()> {
    let metadata = fs::metadata(canonical)
        .with_context(|| format!("failed to inspect watched root '{}'", original.display()))?;
    if metadata.is_dir() {
        return Ok(());
    }

    bail!("watched root '{}' must be a directory", original.display())
}
