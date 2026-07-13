use super::{RuntimeEnvironmentError, RuntimeEnvironmentOptions};
use crate::control_plane::is_valid_environment_variable_key;
use crate::control_plane::state::EnvironmentLifecycle;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// One validated, secret-redacted environment ready for Linux injection.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct RuntimeEnvironment {
    project_id: String,
    managed_revision: String,
    values: BTreeMap<String, String>,
}

impl RuntimeEnvironment {
    pub(crate) fn new(options: RuntimeEnvironmentOptions) -> Result<Self, RuntimeEnvironmentError> {
        let project_id = options.project.as_str();
        if options.managed.project_id() != project_id {
            return Err(RuntimeEnvironmentError::new(format!(
                "project '{project_id}' cannot use managed environment owned by '{}'",
                options.managed.project_id()
            )));
        }
        if options.managed.lifecycle() != EnvironmentLifecycle::Active {
            return Err(RuntimeEnvironmentError::new(format!(
                "project '{project_id}' managed environment is disabled and requires explicit adoption"
            )));
        }
        if options.managed.revision().is_empty() {
            return Err(RuntimeEnvironmentError::new(format!(
                "project '{project_id}' managed environment revision must not be empty"
            )));
        }

        validate_values(project_id, "declared", &options.declared)?;
        validate_values(project_id, "managed", options.managed.values())?;
        let mut values = options.declared;
        for (key, value) in options.managed.values() {
            if values.get(key).is_some_and(|declared| declared != value) {
                return Err(RuntimeEnvironmentError::new(format!(
                    "project '{project_id}' environment key '{key}' conflicts with its daemon-managed value"
                )));
            }
            values.insert(key.clone(), value.clone());
        }

        Ok(Self {
            project_id: project_id.to_owned(),
            managed_revision: options.managed.revision().to_owned(),
            values,
        })
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn managed_revision(&self) -> &str {
        &self.managed_revision
    }

    pub(crate) const fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

impl Debug for RuntimeEnvironment {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeEnvironment")
            .field("project_id", &self.project_id)
            .field("managed_revision", &self.managed_revision)
            .field("value_keys", &self.values.keys())
            .finish()
    }
}

fn validate_values(
    project_id: &str,
    source: &str,
    values: &BTreeMap<String, String>,
) -> Result<(), RuntimeEnvironmentError> {
    for (key, value) in values {
        if !is_valid_environment_variable_key(key) {
            return Err(RuntimeEnvironmentError::new(format!(
                "project '{project_id}' {source} environment key '{key}' is invalid"
            )));
        }
        if value.contains('\0') {
            return Err(RuntimeEnvironmentError::new(format!(
                "project '{project_id}' {source} environment value for '{key}' must not contain NUL bytes"
            )));
        }
    }

    Ok(())
}
