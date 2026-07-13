use super::{PostgresProjectResources, PostgresSharedInstancePlan};

/// One physical PostgreSQL plan and every isolated project tenant it owns.
pub(crate) struct PreparedPostgresSharedInstance {
    instance: PostgresSharedInstancePlan,
    projects: Vec<PostgresProjectResources>,
}

impl PreparedPostgresSharedInstance {
    pub(super) const fn new(
        instance: PostgresSharedInstancePlan,
        projects: Vec<PostgresProjectResources>,
    ) -> Self {
        Self { instance, projects }
    }

    pub(crate) const fn instance(&self) -> &PostgresSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[PostgresProjectResources] {
        &self.projects
    }
}
