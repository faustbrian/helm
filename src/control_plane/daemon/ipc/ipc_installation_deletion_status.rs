use super::IpcInstallationLifecycle;
use serde::{Deserialize, Serialize};

/// Secret-free durable progress for installation teardown polling.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcInstallationDeletionStatus {
    lifecycle: IpcInstallationLifecycle,
    remaining_logical_resources: usize,
    active_operation_ids: Vec<String>,
    failed_operation_id: Option<String>,
    blocking_error: Option<String>,
}

impl IpcInstallationDeletionStatus {
    pub(crate) fn new(
        lifecycle: IpcInstallationLifecycle,
        remaining_logical_resources: usize,
        active_operation_ids: Vec<String>,
        failed_operation_id: Option<String>,
        blocking_error: Option<String>,
    ) -> Result<Self, String> {
        if active_operation_ids.iter().any(String::is_empty)
            || failed_operation_id.as_ref().is_some_and(String::is_empty)
            || blocking_error.as_ref().is_some_and(String::is_empty)
        {
            return Err("installation deletion status fields must not be empty".to_owned());
        }

        Ok(Self {
            lifecycle,
            remaining_logical_resources,
            active_operation_ids,
            failed_operation_id,
            blocking_error,
        })
    }

    pub(crate) const fn lifecycle(&self) -> IpcInstallationLifecycle {
        self.lifecycle
    }

    pub(crate) const fn remaining_logical_resources(&self) -> usize {
        self.remaining_logical_resources
    }

    pub(crate) fn active_operation_ids(&self) -> &[String] {
        &self.active_operation_ids
    }

    pub(crate) fn failed_operation_id(&self) -> Option<&str> {
        self.failed_operation_id.as_deref()
    }

    pub(crate) fn blocking_error(&self) -> Option<&str> {
        self.blocking_error.as_deref()
    }
}
