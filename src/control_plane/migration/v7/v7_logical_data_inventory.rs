use crate::config::ServiceConfig;

/// Non-secret logical names required to select a data migration adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7LogicalDataInventory {
    database: Option<String>,
    bucket: Option<String>,
    region: Option<String>,
}

impl V7LogicalDataInventory {
    pub(super) fn from_service(service: &ServiceConfig) -> Self {
        Self {
            database: service.database.clone(),
            bucket: service.bucket.clone(),
            region: service.region.clone(),
        }
    }

    pub(crate) fn database(&self) -> Option<&str> {
        self.database.as_deref()
    }

    pub(crate) fn bucket(&self) -> Option<&str> {
        self.bucket.as_deref()
    }

    pub(crate) fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }
}
