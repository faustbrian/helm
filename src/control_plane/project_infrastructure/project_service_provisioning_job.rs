use super::ProjectServicePreparationError;
use crate::control_plane::engine::is_immutable_image_identity;
use crate::control_plane::is_valid_environment_variable_key;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// One immutable private-network job required after a service becomes ready.
#[derive(Eq, PartialEq)]
pub(crate) struct ProjectServiceProvisioningJob {
    image: String,
    command: Vec<String>,
    environment: BTreeMap<String, String>,
}

impl ProjectServiceProvisioningJob {
    pub(crate) fn new(
        image: impl Into<String>,
        command: Vec<String>,
        environment: BTreeMap<String, String>,
    ) -> Result<Self, ProjectServicePreparationError> {
        let image = image.into();
        if !is_immutable_image_identity(&image) {
            return Err(invalid(
                "project service provisioning image must use an immutable sha256 digest",
            ));
        }
        if command.first().is_none_or(String::is_empty)
            || command.iter().any(|argument| argument.contains('\0'))
        {
            return Err(invalid(
                "project service provisioning command must be non-empty and contain no NUL bytes",
            ));
        }
        for (key, value) in &environment {
            if !is_valid_environment_variable_key(key) || value.contains('\0') {
                return Err(invalid(format!(
                    "project service provisioning environment key '{key}' is invalid"
                )));
            }
        }

        Ok(Self {
            image,
            command,
            environment,
        })
    }

    pub(crate) fn image(&self) -> &str {
        &self.image
    }

    pub(crate) fn command(&self) -> &[String] {
        &self.command
    }

    pub(crate) const fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }
}

impl Debug for ProjectServiceProvisioningJob {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectServiceProvisioningJob")
            .field("image", &self.image())
            .field("command", &self.command())
            .field("environment_keys", &self.environment().keys())
            .finish()
    }
}

fn invalid(detail: impl Into<String>) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(detail.into())
}
