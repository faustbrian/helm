use super::validate_image_digest::validate_image_digest;
use super::{ProjectProcessPlanOptions, WorkloadPlanError};
use crate::control_plane::is_valid_environment_variable_key;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};

/// One dedicated worker or scheduler process in the project runtime image.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ProjectProcessPlan {
    project_id: String,
    service_id: String,
    container_name: String,
    image_digest: String,
    source_path: PathBuf,
    network_name: String,
    command: Vec<String>,
    environment: BTreeMap<String, String>,
}

impl ProjectProcessPlan {
    pub(crate) fn new(options: ProjectProcessPlanOptions) -> Result<Self, WorkloadPlanError> {
        validate_image_digest("project process", &options.image_digest)?;

        if !options.source_path.is_absolute() {
            return Err(WorkloadPlanError::new(format!(
                "project process source path '{}' must be absolute",
                options.source_path.display()
            )));
        }
        if options.network_name.is_empty() {
            return Err(WorkloadPlanError::new(
                "project process private network name must not be empty",
            ));
        }
        if options.command.first().is_none_or(String::is_empty)
            || options
                .command
                .iter()
                .any(|argument| argument.contains('\0'))
        {
            return Err(WorkloadPlanError::new(
                "project process command must contain a non-empty executable and no NUL bytes",
            ));
        }
        for (key, value) in &options.environment {
            if !is_valid_environment_variable_key(key) {
                return Err(WorkloadPlanError::new(format!(
                    "project process environment key '{key}' is invalid"
                )));
            }
            if value.contains('\0') {
                return Err(WorkloadPlanError::new(format!(
                    "project process environment value for '{key}' must not contain NUL bytes"
                )));
            }
        }

        let project_id = options.project.as_str().to_owned();
        let service_id = options.service.as_str().to_owned();

        Ok(Self {
            container_name: format!("stackctl-{project_id}-{service_id}"),
            project_id,
            service_id,
            image_digest: options.image_digest,
            source_path: options.source_path,
            network_name: options.network_name,
            command: options.command,
            environment: options.environment,
        })
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn container_name(&self) -> &str {
        &self.container_name
    }

    pub(crate) fn image_digest(&self) -> &str {
        &self.image_digest
    }

    pub(crate) fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub(crate) fn network_name(&self) -> &str {
        &self.network_name
    }

    pub(crate) fn command(&self) -> &[String] {
        &self.command
    }

    pub(crate) const fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }
}

impl Debug for ProjectProcessPlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectProcessPlan")
            .field("project_id", &self.project_id)
            .field("service_id", &self.service_id)
            .field("container_name", &self.container_name)
            .field("image_digest", &self.image_digest)
            .field("source_path", &self.source_path)
            .field("network_name", &self.network_name)
            .field("command", &self.command)
            .field("environment_keys", &self.environment.keys())
            .finish()
    }
}
