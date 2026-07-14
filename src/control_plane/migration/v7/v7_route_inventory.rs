/// Exact v7 public route retained until reversible cutover.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct V7RouteInventory {
    service_id: String,
    domain: String,
    scheme: String,
    host_port: u16,
}

impl V7RouteInventory {
    pub(super) fn new(service_id: &str, domain: String, scheme: &str, host_port: u16) -> Self {
        Self {
            service_id: service_id.to_owned(),
            domain,
            scheme: scheme.to_owned(),
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
