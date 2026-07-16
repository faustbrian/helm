use serde::{Deserialize, Serialize};

/// Optional Laravel migration performed after a database restore.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawWorkflowMigration {
    service: String,
    connection: String,
}

impl RawWorkflowMigration {
    pub(crate) fn service(&self) -> &str {
        &self.service
    }

    pub(crate) fn connection(&self) -> &str {
        &self.connection
    }
}
