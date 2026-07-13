use super::MigrationOperationError;
use crate::control_plane::state::{
    EnvironmentLifecycle, LogicalResourceRecord, ManagedEnvironmentRecord, ProjectRecord,
    ResourceLifecycle,
};

/// Exact retained desired state to commit with a rollback checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationRollbackPlan {
    project: ProjectRecord,
    environment: ManagedEnvironmentRecord,
    retained_targets: Vec<LogicalResourceRecord>,
}

impl MigrationRollbackPlan {
    pub(crate) fn new(
        project: ProjectRecord,
        environment: ManagedEnvironmentRecord,
        retained_targets: Vec<LogicalResourceRecord>,
    ) -> Result<Self, MigrationOperationError> {
        if project.project_name() != environment.project_id()
            || environment.lifecycle() != EnvironmentLifecycle::Active
            || retained_targets.iter().any(|target| {
                target.project_id() != project.project_name()
                    || target.lifecycle() != ResourceLifecycle::Retained
            })
        {
            return Err(MigrationOperationError::new(
                "rollback project, active environment, and retained targets do not match",
            ));
        }

        Ok(Self {
            project,
            environment,
            retained_targets,
        })
    }

    pub(crate) const fn project(&self) -> &ProjectRecord {
        &self.project
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }

    pub(crate) fn retained_targets(&self) -> &[LogicalResourceRecord] {
        &self.retained_targets
    }
}
