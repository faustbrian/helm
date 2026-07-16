use super::ProjectLogTarget;

/// Validated ownership targets and Engine options for one log session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectLogRequest {
    session_id: String,
    project_id: String,
    targets: Vec<ProjectLogTarget>,
    follow: bool,
    tail: Option<u32>,
}

impl ProjectLogRequest {
    pub(crate) fn new(
        session_id: String,
        project_id: String,
        targets: Vec<ProjectLogTarget>,
        follow: bool,
        tail: Option<u32>,
    ) -> Self {
        Self {
            session_id,
            project_id,
            targets,
            follow,
            tail,
        }
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    #[cfg(test)]
    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn targets(&self) -> &[ProjectLogTarget] {
        &self.targets
    }

    pub(crate) const fn follow(&self) -> bool {
        self.follow
    }

    pub(crate) const fn tail(&self) -> Option<u32> {
        self.tail
    }
}
