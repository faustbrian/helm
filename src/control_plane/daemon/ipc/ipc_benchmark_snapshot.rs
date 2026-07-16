use super::IpcBenchmarkContainerMetrics;
use serde::{Deserialize, Serialize};

/// One complete same-instant view of owned workload-plane resource usage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcBenchmarkSnapshot {
    observed_at_unix_seconds: i64,
    project_count: usize,
    project_ids: Vec<String>,
    containers: Vec<IpcBenchmarkContainerMetrics>,
}

impl IpcBenchmarkSnapshot {
    pub(crate) fn new(
        observed_at_unix_seconds: i64,
        mut project_ids: Vec<String>,
        mut containers: Vec<IpcBenchmarkContainerMetrics>,
    ) -> Result<Self, String> {
        if observed_at_unix_seconds < 0 {
            return Err("benchmark observation time must not be negative".to_owned());
        }
        project_ids.sort();
        project_ids.dedup();
        if project_ids.iter().any(String::is_empty) {
            return Err("benchmark snapshot contains an empty project ID".to_owned());
        }
        containers.sort_by(|left, right| left.container_id().cmp(right.container_id()));
        if containers
            .windows(2)
            .any(|pair| pair[0].container_id() == pair[1].container_id())
        {
            return Err("benchmark snapshot contains a duplicate container".to_owned());
        }

        Ok(Self {
            observed_at_unix_seconds,
            project_count: project_ids.len(),
            project_ids,
            containers,
        })
    }

    #[cfg(test)]
    pub(crate) const fn observed_at_unix_seconds(&self) -> i64 {
        self.observed_at_unix_seconds
    }

    pub(crate) const fn project_count(&self) -> usize {
        self.project_count
    }

    pub(crate) fn project_ids(&self) -> &[String] {
        &self.project_ids
    }

    pub(crate) fn containers(&self) -> &[IpcBenchmarkContainerMetrics] {
        &self.containers
    }
}
