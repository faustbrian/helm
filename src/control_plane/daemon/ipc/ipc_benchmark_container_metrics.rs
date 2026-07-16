use super::{IpcBenchmarkContainerMetricsOptions, IpcBenchmarkTcpPort};
use serde::{Deserialize, Serialize};

/// Normalized resource sample for one exact owned Engine container.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcBenchmarkContainerMetrics {
    container_id: String,
    resource_kind: String,
    compatibility_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    compatibility_implementation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compatibility_major_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_id: Option<String>,
    cpu_usage_basis_points: u64,
    memory_usage_bytes: u64,
    process_count: u64,
    network_received_bytes: u64,
    network_transmitted_bytes: u64,
    published_tcp_ports: Vec<IpcBenchmarkTcpPort>,
}

impl IpcBenchmarkContainerMetrics {
    pub(crate) fn new(options: IpcBenchmarkContainerMetricsOptions) -> Result<Self, String> {
        let IpcBenchmarkContainerMetricsOptions {
            container_id,
            resource_kind,
            compatibility_fingerprint,
            compatibility_implementation,
            compatibility_major_version,
            project_id,
            resource_id,
            cpu_usage_basis_points,
            memory_usage_bytes,
            process_count,
            network_received_bytes,
            network_transmitted_bytes,
            published_tcp_ports,
        } = options;

        if container_id.is_empty()
            || resource_kind.is_empty()
            || compatibility_fingerprint.is_empty()
        {
            return Err("benchmark container identity must not be empty".to_owned());
        }
        if compatibility_implementation.is_some() != compatibility_major_version.is_some() {
            return Err(
                "benchmark compatibility implementation and major version must be paired"
                    .to_owned(),
            );
        }

        Ok(Self {
            container_id,
            resource_kind,
            compatibility_fingerprint,
            compatibility_implementation,
            compatibility_major_version,
            project_id,
            resource_id,
            cpu_usage_basis_points,
            memory_usage_bytes,
            process_count,
            network_received_bytes,
            network_transmitted_bytes,
            published_tcp_ports,
        })
    }

    pub(crate) fn container_id(&self) -> &str {
        &self.container_id
    }

    pub(crate) fn resource_kind(&self) -> &str {
        &self.resource_kind
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }

    pub(crate) fn compatibility_implementation(&self) -> Option<&str> {
        self.compatibility_implementation.as_deref()
    }

    pub(crate) fn compatibility_major_version(&self) -> Option<&str> {
        self.compatibility_major_version.as_deref()
    }

    pub(crate) fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub(crate) fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    #[cfg(test)]
    pub(crate) const fn cpu_usage_basis_points(&self) -> u64 {
        self.cpu_usage_basis_points
    }

    #[cfg(test)]
    pub(crate) const fn memory_usage_bytes(&self) -> u64 {
        self.memory_usage_bytes
    }

    #[cfg(test)]
    pub(crate) const fn process_count(&self) -> u64 {
        self.process_count
    }

    #[cfg(test)]
    pub(crate) const fn network_received_bytes(&self) -> u64 {
        self.network_received_bytes
    }

    #[cfg(test)]
    pub(crate) const fn network_transmitted_bytes(&self) -> u64 {
        self.network_transmitted_bytes
    }

    #[cfg(test)]
    pub(crate) fn published_tcp_ports(&self) -> &[IpcBenchmarkTcpPort] {
        &self.published_tcp_ports
    }
}
