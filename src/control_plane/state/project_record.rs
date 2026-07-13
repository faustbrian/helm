use std::path::{Path, PathBuf};

/// Durable project identity and its complete deterministic route ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectRecord {
    canonical_path: PathBuf,
    project_name: String,
    route_domains: Vec<String>,
}

impl ProjectRecord {
    /// Creates a record with sorted, deduplicated route domains.
    pub(crate) fn new(
        canonical_path: PathBuf,
        project_name: String,
        mut route_domains: Vec<String>,
    ) -> Self {
        route_domains.sort();
        route_domains.dedup();

        Self {
            canonical_path,
            project_name,
            route_domains,
        }
    }

    /// Returns the canonical project path.
    pub(crate) fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    /// Returns the exact validated project name.
    pub(crate) fn project_name(&self) -> &str {
        &self.project_name
    }

    /// Returns every route domain owned by this project.
    pub(crate) fn route_domains(&self) -> &[String] {
        &self.route_domains
    }
}
