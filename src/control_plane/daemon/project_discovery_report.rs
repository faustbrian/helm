use super::ProjectDiscoveryIssue;
use crate::control_plane::application::ProjectSource;

/// Deterministic valid sources and isolated diagnostics from one complete scan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectDiscoveryReport {
    sources: Vec<ProjectSource>,
    issues: Vec<ProjectDiscoveryIssue>,
}

impl ProjectDiscoveryReport {
    pub(super) fn new(sources: Vec<ProjectSource>, issues: Vec<ProjectDiscoveryIssue>) -> Self {
        Self { sources, issues }
    }

    pub(crate) fn sources(&self) -> &[ProjectSource] {
        &self.sources
    }

    pub(crate) fn issues(&self) -> &[ProjectDiscoveryIssue] {
        &self.issues
    }
}
