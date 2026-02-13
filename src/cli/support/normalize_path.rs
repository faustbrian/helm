//! cli support normalize path module.
//!
//! Contains cli support normalize path logic used by Helm command workflows.

/// Normalizes path into a canonical form.
pub(crate) fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}
