use crate::cli::args::BenchmarkEvidenceScenario;
use crate::control_plane::IpcBenchmarkSnapshot;
use anyhow::{Result, bail};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_benchmark_scenario(
    scenario: BenchmarkEvidenceScenario,
    snapshot: &IpcBenchmarkSnapshot,
) -> Result<()> {
    let (
        label,
        expected_projects,
        expected_application_fingerprints,
        expected_shared_instances,
        expected_containers,
    ) = match scenario {
        BenchmarkEvidenceScenario::One => ("v8-one", 1, 1, 4, 7),
        BenchmarkEvidenceScenario::FortyCompatible => ("v8-forty-compatible", 40, 1, 4, 85),
        BenchmarkEvidenceScenario::FortySplit => ("v8-forty-split", 40, 2, 5, 86),
    };

    let mut application_projects = BTreeSet::new();
    let mut application_fingerprints = BTreeSet::new();
    let mut process_projects = BTreeSet::new();
    let mut shared_fingerprints = BTreeSet::new();
    let mut shared_profiles = BTreeMap::<&str, BTreeSet<&str>>::new();
    let mut applications = 0;
    let mut processes = 0;
    let mut shared_instances = 0;
    let mut gateways = 0;

    for container in snapshot.containers() {
        match container.resource_kind() {
            "project_application" => {
                applications += 1;
                let Some(project_id) = container.project_id() else {
                    bail!("benchmark scenario '{label}' has an application without a project ID");
                };
                application_projects.insert(project_id);
                application_fingerprints.insert(container.compatibility_fingerprint());
            }
            "project_process" => {
                processes += 1;
                let Some(project_id) = container.project_id() else {
                    bail!("benchmark scenario '{label}' has a process without a project ID");
                };
                if container.resource_id() != Some("worker") {
                    bail!(
                        "benchmark scenario '{label}' requires the canonical worker process for project '{project_id}', observed resource {:?}",
                        container.resource_id()
                    );
                }
                process_projects.insert(project_id);
            }
            "shared_service" => {
                shared_instances += 1;
                shared_fingerprints.insert(container.compatibility_fingerprint());
                let Some(implementation) = container.compatibility_implementation() else {
                    bail!(
                        "benchmark scenario '{label}' has a shared service without a compatibility implementation"
                    );
                };
                let Some(major_version) = container.compatibility_major_version() else {
                    bail!(
                        "benchmark scenario '{label}' has a shared service without a compatibility major version"
                    );
                };
                shared_profiles
                    .entry(implementation)
                    .or_default()
                    .insert(major_version);
            }
            "gateway" => gateways += 1,
            kind => {
                bail!("benchmark scenario '{label}' contains unexpected container kind '{kind}'")
            }
        }
    }

    let registered_projects = snapshot
        .project_ids()
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if registered_projects.len() != snapshot.project_count()
        || application_projects != registered_projects
        || process_projects != registered_projects
    {
        bail!(
            "benchmark scenario '{label}' project ownership mismatch: registered {registered_projects:?}, applications {application_projects:?}, processes {process_projects:?}"
        );
    }

    validate_shared_profiles(label, scenario, &shared_profiles)?;

    let actual = (
        snapshot.project_count(),
        applications,
        application_projects.len(),
        application_fingerprints.len(),
        processes,
        process_projects.len(),
        shared_instances,
        shared_fingerprints.len(),
        gateways,
        snapshot.containers().len(),
    );
    let expected = (
        expected_projects,
        expected_projects,
        expected_projects,
        expected_application_fingerprints,
        expected_projects,
        expected_projects,
        expected_shared_instances,
        expected_shared_instances,
        1,
        expected_containers,
    );
    if actual != expected {
        bail!(
            "benchmark scenario '{label}' topology mismatch: expected \
             projects/apps/app-projects/app-fingerprints/processes/process-projects/shared/shared-fingerprints/gateways/containers \
             {expected:?}, observed {actual:?}"
        );
    }

    Ok(())
}

fn validate_shared_profiles(
    label: &str,
    scenario: BenchmarkEvidenceScenario,
    profiles: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<()> {
    let expected_implementations = BTreeMap::from([
        ("mailpit", 1),
        ("minio", 1),
        (
            "postgresql",
            if scenario == BenchmarkEvidenceScenario::FortySplit {
                2
            } else {
                1
            },
        ),
        ("valkey", 1),
    ]);
    let actual_implementations = profiles
        .iter()
        .map(|(implementation, majors)| (*implementation, majors.len()))
        .collect::<BTreeMap<_, _>>();
    let expected_postgres_majors = if scenario == BenchmarkEvidenceScenario::FortySplit {
        BTreeSet::from(["17", "18"])
    } else {
        BTreeSet::from(["17"])
    };
    if actual_implementations != expected_implementations
        || profiles.get("postgresql") != Some(&expected_postgres_majors)
    {
        bail!(
            "benchmark scenario '{label}' shared service profiles mismatch: expected implementations {expected_implementations:?} with PostgreSQL majors {expected_postgres_majors:?}, observed {profiles:?}"
        );
    }

    Ok(())
}
