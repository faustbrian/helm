use super::ProjectServicePreparationError;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use std::fmt::{Debug, Formatter};

/// One generated private file mounted read-only into a project service.
#[derive(Eq, PartialEq)]
pub(crate) struct ProjectServiceContainerConfiguration {
    file_name: String,
    mount_target: String,
    contents: CredentialSecret,
}

impl ProjectServiceContainerConfiguration {
    pub(crate) fn new(
        file_name: impl Into<String>,
        mount_target: impl Into<String>,
        contents: CredentialSecret,
    ) -> Result<Self, ProjectServicePreparationError> {
        let file_name = file_name.into();
        let mount_target = mount_target.into();
        if file_name.is_empty()
            || file_name == "."
            || file_name == ".."
            || file_name.contains('/')
            || file_name.contains('\\')
        {
            return Err(invalid(
                "project service configuration requires one plain file name",
            ));
        }
        if !mount_target.starts_with('/') || mount_target.ends_with('/') {
            return Err(invalid(
                "project service configuration requires an absolute file mount target",
            ));
        }
        if contents.expose().is_empty() || contents.expose().contains('\0') {
            return Err(invalid(
                "project service configuration contents must be non-empty and contain no NUL bytes",
            ));
        }

        Ok(Self {
            file_name,
            mount_target,
            contents,
        })
    }

    pub(crate) fn file_name(&self) -> &str {
        &self.file_name
    }

    pub(crate) fn mount_target(&self) -> &str {
        &self.mount_target
    }

    pub(super) const fn contents(&self) -> &CredentialSecret {
        &self.contents
    }
}

impl Debug for ProjectServiceContainerConfiguration {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectServiceContainerConfiguration")
            .field("file_name", &self.file_name())
            .field("mount_target", &self.mount_target())
            .field("contents", &"[REDACTED]")
            .finish()
    }
}

fn invalid(detail: impl Into<String>) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(detail.into())
}
