use serde::{Deserialize, Serialize};

/// Exact legacy route retained during reversible migration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcV7Route {
    service_id: String,
    domain: String,
    scheme: String,
    host_port: u16,
}

impl IpcV7Route {
    pub(crate) const fn new(
        service_id: String,
        domain: String,
        scheme: String,
        host_port: u16,
    ) -> Self {
        Self {
            service_id,
            domain,
            scheme,
            host_port,
        }
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn domain(&self) -> &str {
        &self.domain
    }

    pub(crate) fn scheme(&self) -> &str {
        &self.scheme
    }

    pub(crate) const fn host_port(&self) -> u16 {
        self.host_port
    }
}
