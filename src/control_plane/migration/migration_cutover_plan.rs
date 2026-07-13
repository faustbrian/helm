use super::MigrationOperationError;
use crate::control_plane::state::{EnvironmentLifecycle, ManagedEnvironmentRecord, ProjectRecord};

/// Exact desired project state to commit with a cutover checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationCutoverPlan {
    project: ProjectRecord,
    environment: ManagedEnvironmentRecord,
}

impl MigrationCutoverPlan {
    pub(crate) fn new(
        project: ProjectRecord,
        environment: ManagedEnvironmentRecord,
    ) -> Result<Self, MigrationOperationError> {
        if project.project_name() != environment.project_id()
            || environment.lifecycle() != EnvironmentLifecycle::Active
        {
            return Err(MigrationOperationError::new(
                "cutover project and active managed environment do not match",
            ));
        }

        Ok(Self {
            project,
            environment,
        })
    }

    pub(crate) const fn project(&self) -> &ProjectRecord {
        &self.project
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
