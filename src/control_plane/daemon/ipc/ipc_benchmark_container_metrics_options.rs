use super::IpcBenchmarkTcpPort;

/// Complete inputs for one normalized Engine resource sample.
pub(crate) struct IpcBenchmarkContainerMetricsOptions {
    pub(crate) container_id: String,
    pub(crate) resource_kind: String,
    pub(crate) compatibility_fingerprint: String,
    pub(crate) compatibility_implementation: Option<String>,
    pub(crate) compatibility_major_version: Option<String>,
    pub(crate) project_id: Option<String>,
    pub(crate) resource_id: Option<String>,
    pub(crate) cpu_usage_basis_points: u64,
    pub(crate) memory_usage_bytes: u64,
    pub(crate) process_count: u64,
    pub(crate) network_received_bytes: u64,
    pub(crate) network_transmitted_bytes: u64,
    pub(crate) published_tcp_ports: Vec<IpcBenchmarkTcpPort>,
}
