use super::EngineError;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// Validated non-shell command to execute inside an owned container.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct CommandRequest {
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    working_directory: Option<String>,
}

impl CommandRequest {
    pub(crate) fn new(
        arguments: Vec<String>,
        environment: BTreeMap<String, String>,
        working_directory: Option<String>,
    ) -> Result<Self, EngineError> {
        if arguments.first().is_none_or(String::is_empty) {
            return Err(EngineError::InvalidRequest {
                detail: "container command must not be empty".to_owned(),
            });
        }

        if arguments.iter().any(|argument| argument.contains('\0')) {
            return Err(EngineError::InvalidRequest {
                detail: "container command arguments must not contain NUL bytes".to_owned(),
            });
        }

        for (key, value) in &environment {
            if key.is_empty() || key.contains(['=', '\0']) {
                return Err(EngineError::InvalidRequest {
                    detail: format!("container command environment key '{key}' is invalid"),
                });
            }

            if value.contains('\0') {
                return Err(EngineError::InvalidRequest {
                    detail: format!(
                        "container command environment value for '{key}' must not contain NUL bytes"
                    ),
                });
            }
        }

        if let Some(directory) = &working_directory {
            if !directory.starts_with('/') {
                return Err(EngineError::InvalidRequest {
                    detail: format!(
                        "container command working directory '{directory}' must be absolute"
                    ),
                });
            }

            if directory.contains('\0') {
                return Err(EngineError::InvalidRequest {
                    detail: "container command working directory must not contain NUL bytes"
                        .to_owned(),
                });
            }
        }

        Ok(Self {
            arguments,
            environment,
            working_directory,
        })
    }

    pub(crate) fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub(super) const fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    pub(super) fn working_directory(&self) -> Option<&str> {
        self.working_directory.as_deref()
    }
}

impl Debug for CommandRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandRequest")
            .field("argument_count", &self.arguments.len())
            .field("environment_keys", &self.environment.keys())
            .field("working_directory", &self.working_directory)
            .finish()
    }
}
