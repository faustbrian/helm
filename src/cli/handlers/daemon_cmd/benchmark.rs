use anyhow::{Result, bail};

#[cfg(unix)]
pub(super) fn handle_daemon_benchmark(args: &crate::cli::args::DaemonBenchmarkArgs) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};
    use std::io::Write as _;

    let response = super::send_singleton_request(IpcPayload::BenchmarkSnapshot {
        require_converged: args.evidence_scenario.is_some(),
    })?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::BenchmarkSnapshot { snapshot },
        } => {
            super::validate_benchmark_project_count::validate_benchmark_project_count(
                args.expect_projects,
                snapshot.project_count(),
            )?;
            if let Some(scenario) = args.evidence_scenario {
                super::validate_benchmark_scenario::validate_benchmark_scenario(
                    scenario, snapshot,
                )?;
            }
            let stdout = std::io::stdout();
            let mut output = stdout.lock();
            serde_json::to_writer_pretty(&mut output, snapshot)?;
            output.write_all(b"\n")?;
            output.flush()?;

            Ok(())
        }
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected benchmark response"),
        IpcOutcome::Failure { diagnostics } => {
            let message = diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
                .collect::<Vec<_>>()
                .join("; ");
            bail!("benchmark snapshot failed: {message}")
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::args::BenchmarkEvidenceScenario;
    use crate::control_plane::IpcBenchmarkSnapshot;
    use serde_json::{Value, json};

    use super::super::validate_benchmark_project_count::validate_benchmark_project_count;
    use super::super::validate_benchmark_scenario::validate_benchmark_scenario;

    #[test]
    fn expected_project_count_rejects_a_mislabeled_sample() {
        let error = validate_benchmark_project_count(Some(40), 39)
            .expect_err("mislabeled forty-project sample must fail");

        assert_eq!(
            error.to_string(),
            "benchmark expected exactly 40 projects, but the daemon reported 39"
        );
    }

    #[test]
    fn compatible_fixture_cannot_be_labeled_as_split() {
        let snapshot = benchmark_snapshot(
            40,
            &["app-v1"],
            &["postgresql:17", "valkey:8", "minio:1", "mailpit:1"],
        );
        let error = validate_benchmark_scenario(BenchmarkEvidenceScenario::V8FortySplit, &snapshot)
            .expect_err("compatible topology must not pass as split");

        assert!(error.to_string().contains("v8-forty-split"));
    }

    #[test]
    fn canonical_evidence_topologies_pass() {
        let fixtures = [
            (
                BenchmarkEvidenceScenario::V8One,
                benchmark_snapshot(
                    1,
                    &["app-v1"],
                    &["postgresql:17", "valkey:8", "minio:1", "mailpit:1"],
                ),
            ),
            (
                BenchmarkEvidenceScenario::V8FortyCompatible,
                benchmark_snapshot(
                    40,
                    &["app-v1"],
                    &["postgresql:17", "valkey:8", "minio:1", "mailpit:1"],
                ),
            ),
            (
                BenchmarkEvidenceScenario::V8FortySplit,
                benchmark_snapshot(
                    40,
                    &["app-v1", "app-v2"],
                    &[
                        "postgresql:17",
                        "postgresql:18",
                        "valkey:8",
                        "minio:1",
                        "mailpit:1",
                    ],
                ),
            ),
        ];

        for (scenario, snapshot) in fixtures {
            assert!(validate_benchmark_scenario(scenario, &snapshot).is_ok());
        }
    }

    #[test]
    fn evidence_rejects_disjoint_registered_and_container_project_ids() {
        let mut value = benchmark_snapshot_value(
            1,
            &["app-v1"],
            &["postgresql:17", "valkey:8", "minio:1", "mailpit:1"],
        );
        value["containers"][1]["project_id"] = json!("removed-project");
        let snapshot = serde_json::from_value(value).expect("benchmark snapshot fixture");

        let error = validate_benchmark_scenario(BenchmarkEvidenceScenario::V8One, &snapshot)
            .expect_err("container ownership must equal registered project ownership");

        assert!(error.to_string().contains("project ownership"));
    }

    #[test]
    fn split_evidence_requires_postgresql_17_and_18() {
        let snapshot = benchmark_snapshot(
            40,
            &["app-v1", "app-v2"],
            &[
                "postgresql:17",
                "valkey:7",
                "valkey:8",
                "minio:1",
                "mailpit:1",
            ],
        );

        let error = validate_benchmark_scenario(BenchmarkEvidenceScenario::V8FortySplit, &snapshot)
            .expect_err("unrelated service splits must not substitute for PostgreSQL majors");

        assert!(error.to_string().contains("shared service profiles"));
    }

    #[test]
    fn evidence_rejects_scheduler_substitution_for_worker() {
        let mut value = benchmark_snapshot_value(
            1,
            &["app-v1"],
            &["postgresql:17", "valkey:8", "minio:1", "mailpit:1"],
        );
        value["containers"][1]["resource_id"] = json!("scheduler");
        let snapshot = serde_json::from_value(value).expect("benchmark snapshot fixture");

        let error = validate_benchmark_scenario(BenchmarkEvidenceScenario::V8One, &snapshot)
            .expect_err("a scheduler must not substitute for the canonical worker");

        assert!(error.to_string().contains("worker process"));
    }

    fn benchmark_snapshot(
        project_count: usize,
        application_fingerprints: &[&str],
        shared_fingerprints: &[&str],
    ) -> IpcBenchmarkSnapshot {
        serde_json::from_value(benchmark_snapshot_value(
            project_count,
            application_fingerprints,
            shared_fingerprints,
        ))
        .expect("benchmark snapshot fixture")
    }

    fn benchmark_snapshot_value(
        project_count: usize,
        application_fingerprints: &[&str],
        shared_profiles: &[&str],
    ) -> Value {
        let mut containers = Vec::new();
        let project_ids = (0..project_count)
            .map(|project| format!("project-{project}"))
            .collect::<Vec<_>>();
        for project in 0..project_count {
            let project_id = &project_ids[project];
            let fingerprint = application_fingerprints[project % application_fingerprints.len()];
            containers.push(container(
                &format!("app-{project}"),
                "project_application",
                fingerprint,
                Some(project_id),
                None,
            ));
            let mut worker = container(
                &format!("worker-{project}"),
                "project_process",
                fingerprint,
                Some(project_id),
                None,
            );
            worker["resource_id"] = json!("worker");
            containers.push(worker);
        }
        containers.push(container("gateway", "gateway", "gateway-v1", None, None));
        for (index, profile) in shared_profiles.iter().enumerate() {
            let (implementation, major_version) = profile
                .split_once(':')
                .expect("shared fixture profile has implementation and major");
            containers.push(container(
                &format!("shared-{index}"),
                "shared_service",
                &format!("sha256:{profile}"),
                None,
                Some((implementation, major_version)),
            ));
        }

        json!({
            "observed_at_unix_seconds": 10_000,
            "project_count": project_count,
            "project_ids": project_ids,
            "containers": containers,
        })
    }

    fn container(
        container_id: &str,
        resource_kind: &str,
        compatibility_fingerprint: &str,
        project_id: Option<&str>,
        shared_profile: Option<(&str, &str)>,
    ) -> Value {
        let mut value = json!({
            "container_id": container_id,
            "resource_kind": resource_kind,
            "compatibility_fingerprint": compatibility_fingerprint,
            "cpu_usage_basis_points": 1,
            "memory_usage_bytes": 1,
            "process_count": 1,
            "network_received_bytes": 1,
            "network_transmitted_bytes": 1,
            "published_tcp_ports": [],
        });
        if let Some(project_id) = project_id {
            value["project_id"] = json!(project_id);
        }
        if let Some((implementation, major_version)) = shared_profile {
            value["compatibility_implementation"] = json!(implementation);
            value["compatibility_major_version"] = json!(major_version);
        }

        value
    }
}
