use super::ipc::{
    IpcBenchmarkContainerMetrics, IpcBenchmarkContainerMetricsOptions, IpcBenchmarkSnapshot,
    IpcBenchmarkTcpPort,
};
use crate::control_plane::engine::{
    ContainerDiscovery, ObservedResourceOwnership, PublishedPortDiscovery, ResourceMetrics,
    reconstruct_owned_container,
};
use std::collections::BTreeMap;

/// Collects one complete ownership-scoped sample without Engine mutation.
pub(crate) async fn collect_benchmark_snapshot<E>(
    engine: &E,
    installation_id: &str,
    schema_version: u32,
    project_ids: Vec<String>,
    observed_at_unix_seconds: i64,
) -> Result<IpcBenchmarkSnapshot, String>
where
    E: ContainerDiscovery + PublishedPortDiscovery + ResourceMetrics,
{
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| format!("failed to discover benchmark containers: {error}"))?;
    let published = engine
        .discover_published_tcp_ports()
        .await
        .map_err(|error| format!("failed to discover benchmark ports: {error}"))?;
    let mut ports = BTreeMap::<String, Vec<IpcBenchmarkTcpPort>>::new();
    for binding in published {
        ports
            .entry(binding.container_id().as_str().to_owned())
            .or_default()
            .push(IpcBenchmarkTcpPort::new(
                binding.host_ip().to_string(),
                binding.host_port(),
            )?);
    }

    let mut containers = Vec::new();
    for observed in observed {
        let owned = match reconstruct_owned_container(&observed, installation_id, schema_version) {
            Ok(owned) => owned,
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => continue,
            Err(ownership) => {
                return Err(format!(
                    "managed benchmark container '{}' has ambiguous ownership: {ownership:?}",
                    observed.id().as_str()
                ));
            }
        };
        let sample = engine.sample_resources(&owned).await.map_err(|error| {
            format!(
                "failed to sample benchmark container '{}': {error}",
                owned.id().as_str()
            )
        })?;
        let mut published_tcp_ports = ports.remove(owned.id().as_str()).unwrap_or_default();
        published_tcp_ports.sort_by(|left, right| {
            (left.host_ip(), left.host_port()).cmp(&(right.host_ip(), right.host_port()))
        });
        containers.push(IpcBenchmarkContainerMetrics::new(
            IpcBenchmarkContainerMetricsOptions {
                container_id: owned.id().as_str().to_owned(),
                resource_kind: owned.metadata().kind().label().to_owned(),
                compatibility_fingerprint: owned.metadata().compatibility_fingerprint().to_owned(),
                compatibility_implementation: owned
                    .metadata()
                    .compatibility_implementation()
                    .map(str::to_owned),
                compatibility_major_version: owned
                    .metadata()
                    .compatibility_major_version()
                    .map(str::to_owned),
                project_id: owned.metadata().project_id().map(str::to_owned),
                resource_id: owned.metadata().resource_id().map(str::to_owned),
                cpu_usage_basis_points: required_metric(
                    "CPU usage",
                    owned.id().as_str(),
                    sample.cpu_usage_basis_points(),
                )?,
                memory_usage_bytes: required_metric(
                    "memory usage",
                    owned.id().as_str(),
                    sample.memory_usage_bytes(),
                )?,
                process_count: required_metric(
                    "process count",
                    owned.id().as_str(),
                    sample.process_count(),
                )?,
                network_received_bytes: required_metric(
                    "network received bytes",
                    owned.id().as_str(),
                    sample.network_received_bytes(),
                )?,
                network_transmitted_bytes: required_metric(
                    "network transmitted bytes",
                    owned.id().as_str(),
                    sample.network_transmitted_bytes(),
                )?,
                published_tcp_ports,
            },
        )?);
    }

    IpcBenchmarkSnapshot::new(observed_at_unix_seconds, project_ids, containers)
}

fn required_metric(name: &str, container_id: &str, value: Option<u64>) -> Result<u64, String> {
    value.ok_or_else(|| {
        format!("benchmark container '{container_id}' did not report required {name}")
    })
}

#[cfg(test)]
mod tests {
    use super::required_metric;

    #[test]
    fn benchmark_metrics_fail_closed_when_the_engine_omits_a_value() {
        let error = required_metric("memory usage", "container-app", None)
            .expect_err("missing metric must fail");

        assert_eq!(
            error,
            "benchmark container 'container-app' did not report required memory usage"
        );
    }
}
