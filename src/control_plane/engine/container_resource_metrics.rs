/// One normalized Engine resource sample for an owned container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContainerResourceMetrics {
    cpu_usage_basis_points: Option<u64>,
    memory_usage_bytes: Option<u64>,
    process_count: Option<u64>,
    network_received_bytes: Option<u64>,
    network_transmitted_bytes: Option<u64>,
}

impl ContainerResourceMetrics {
    pub(crate) const fn new(
        cpu_usage_basis_points: Option<u64>,
        memory_usage_bytes: Option<u64>,
        process_count: Option<u64>,
        network_received_bytes: Option<u64>,
        network_transmitted_bytes: Option<u64>,
    ) -> Self {
        Self {
            cpu_usage_basis_points,
            memory_usage_bytes,
            process_count,
            network_received_bytes,
            network_transmitted_bytes,
        }
    }

    pub(crate) const fn cpu_usage_basis_points(&self) -> Option<u64> {
        self.cpu_usage_basis_points
    }

    pub(crate) const fn memory_usage_bytes(&self) -> Option<u64> {
        self.memory_usage_bytes
    }

    pub(crate) const fn process_count(&self) -> Option<u64> {
        self.process_count
    }

    pub(crate) const fn network_received_bytes(&self) -> Option<u64> {
        self.network_received_bytes
    }

    pub(crate) const fn network_transmitted_bytes(&self) -> Option<u64> {
        self.network_transmitted_bytes
    }
}
