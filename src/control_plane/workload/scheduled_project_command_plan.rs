use super::{
    ProjectCommand, ProjectCommandPlan, ProjectCommandPlanOptions,
    ScheduledProjectCommandPlanOptions,
};
use crate::control_plane::engine::EngineError;
use crate::control_plane::{ProjectIdentity, ServiceIdentity};
use std::collections::BTreeMap;
use std::time::Duration;

/// One daemon-timed non-shell command bound to an exact project application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScheduledProjectCommandPlan {
    project: ProjectIdentity,
    service: ServiceIdentity,
    application_service: ServiceIdentity,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    timeout: Duration,
}

impl ScheduledProjectCommandPlan {
    pub(crate) fn new(options: ScheduledProjectCommandPlanOptions) -> Result<Self, EngineError> {
        let plan = Self {
            project: options.project,
            service: options.service,
            application_service: options.application_service,
            arguments: options.arguments,
            environment: options.environment,
            timeout: options.timeout,
        };
        plan.command_plan()?;

        Ok(plan)
    }

    pub(crate) fn project_id(&self) -> &str {
        self.project.as_str()
    }

    pub(crate) fn service_id(&self) -> &str {
        self.service.as_str()
    }

    pub(crate) fn application_service(&self) -> &str {
        self.application_service.as_str()
    }

    pub(crate) fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub(crate) const fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    pub(crate) fn command_plan(&self) -> Result<ProjectCommandPlan, EngineError> {
        ProjectCommandPlan::new(ProjectCommandPlanOptions {
            project: self.project.clone(),
            command: ProjectCommand::Exec {
                arguments: self.arguments.clone(),
            },
            environment: self.environment.clone(),
            input: Vec::new(),
            timeout: self.timeout,
            browser_session: false,
        })
    }
}
