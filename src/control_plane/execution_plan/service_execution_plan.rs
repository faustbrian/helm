use crate::control_plane::{
    DesiredService, ProjectIdentity, ServiceDeploymentStrategy, ServiceIdentity,
};
use std::path::{Path, PathBuf};

/// One desired service resolved to its explicit v8 deployment strategy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceExecutionPlan {
    project: ProjectIdentity,
    project_directory: PathBuf,
    service: ServiceIdentity,
    strategy: ServiceDeploymentStrategy,
    desired: DesiredService,
}

impl ServiceExecutionPlan {
    pub(super) fn new(
        project: ProjectIdentity,
        project_directory: PathBuf,
        service: ServiceIdentity,
        strategy: ServiceDeploymentStrategy,
        desired: DesiredService,
    ) -> Self {
        Self {
            project,
            project_directory,
            service,
            strategy,
            desired,
        }
    }

    pub(crate) const fn project(&self) -> &ProjectIdentity {
        &self.project
    }

    pub(crate) fn project_directory(&self) -> &Path {
        &self.project_directory
    }

    pub(crate) const fn service(&self) -> &ServiceIdentity {
        &self.service
    }

    pub(crate) const fn strategy(&self) -> ServiceDeploymentStrategy {
        self.strategy
    }

    pub(crate) const fn desired(&self) -> &DesiredService {
        &self.desired
    }
}
