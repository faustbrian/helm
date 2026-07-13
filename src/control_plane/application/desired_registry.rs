use crate::control_plane::DesiredProject;

/// A complete discovered registry validated before any persistence or effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesiredRegistry {
    projects: Vec<DesiredProject>,
}

impl DesiredRegistry {
    pub(super) fn new(projects: Vec<DesiredProject>) -> Self {
        Self { projects }
    }

    /// Returns projects in canonical-path order.
    pub(crate) fn projects(&self) -> &[DesiredProject] {
        &self.projects
    }
}
