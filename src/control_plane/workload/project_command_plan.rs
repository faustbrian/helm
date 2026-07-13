use super::ProjectCommandPlanOptions;
use crate::control_plane::engine::{AttachedCommandOptions, CommandRequest, EngineError};
use std::fmt::{Debug, Formatter};

const PROJECT_WORKING_DIRECTORY: &str = "/workspace";

/// A bounded non-shell command ready for direct Engine exec.
pub(crate) struct ProjectCommandPlan {
    project_id: String,
    arguments: Vec<String>,
    action: String,
    attached: AttachedCommandOptions,
}

impl Debug for ProjectCommandPlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectCommandPlan")
            .field("project_id", &self.project_id)
            .field("argument_count", &self.arguments.len())
            .field("action", &self.action)
            .finish()
    }
}

impl ProjectCommandPlan {
    pub(crate) fn new(options: ProjectCommandPlanOptions) -> Result<Self, EngineError> {
        let project_id = options.project.as_str().to_owned();
        let (command_name, arguments) = options
            .command
            .into_parts()
            .map_err(|detail| EngineError::InvalidRequest { detail })?;
        let action = format!("run {command_name} for project '{project_id}'");
        let request = CommandRequest::new(
            arguments.clone(),
            options.environment,
            Some(PROJECT_WORKING_DIRECTORY.to_owned()),
        )?;
        let attached =
            AttachedCommandOptions::new(request, options.input, action.clone(), options.timeout)?;

        Ok(Self {
            project_id,
            arguments,
            action,
            attached,
        })
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub(crate) const fn working_directory(&self) -> &'static str {
        PROJECT_WORKING_DIRECTORY
    }

    pub(crate) fn action(&self) -> &str {
        &self.action
    }

    pub(super) const fn attached(&self) -> &AttachedCommandOptions {
        &self.attached
    }
}
