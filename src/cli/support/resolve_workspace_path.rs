//! Workspace-relative path resolution helpers.

use std::path::{Path, PathBuf};

/// Resolves `path` relative to `workspace_root` unless already absolute.
pub(crate) fn resolve_workspace_path<P>(workspace_root: &Path, path: P) -> PathBuf
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if path.is_absolute() {
        return path.to_path_buf();
    }
    workspace_root.join(path)
}
