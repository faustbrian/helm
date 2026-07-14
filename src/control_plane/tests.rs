use super::{
    KNOWN_SERVICE_PRESETS, PRESET_ARTIFACT_CATALOG_REVISION, ProjectIdentity, RouteClaim,
    RouteIdentity, ServiceDeploymentStrategy, ServiceIdentity, resolve_preset_artifact,
    resolve_service_deployment_strategy, validate_route_claims,
};
use std::path::{Path, PathBuf};

#[test]
fn explicit_project_name_is_preserved_exactly() {
    let project = ProjectIdentity::resolve(Some("bill-1"), Path::new("/work/bill"))
        .expect("valid explicit project name");

    assert_eq!(project.as_str(), "bill-1");
}

#[test]
fn directory_basename_is_used_when_project_name_is_absent() {
    let project = ProjectIdentity::resolve(None, Path::new("/work/bill-2"))
        .expect("valid directory project name");

    assert_eq!(project.as_str(), "bill-2");
}

#[test]
fn project_name_is_rejected_instead_of_normalized() {
    let error = ProjectIdentity::resolve(Some("Billing_API"), Path::new("/work/bill"))
        .expect_err("invalid project name must fail");

    assert_eq!(
        error.to_string(),
        "project name 'Billing_API' must be a valid lowercase DNS label"
    );
}

#[test]
fn service_name_is_rejected_instead_of_normalized() {
    let error = ServiceIdentity::new("Mail Pit").expect_err("invalid service name must fail");

    assert_eq!(
        error.to_string(),
        "service name 'Mail Pit' must be a valid lowercase DNS label"
    );
}

#[test]
fn every_route_includes_the_project_and_service_names() {
    let project = ProjectIdentity::resolve(Some("bill"), Path::new("/work/ignored"))
        .expect("valid project name");
    let service = ServiceIdentity::new("app").expect("valid service name");
    let route = RouteIdentity::new(&project, &service).expect("valid route");

    assert_eq!(route.domain(), "bill-app.stackctl.localhost");
}

#[test]
fn combined_route_label_longer_than_63_bytes_is_rejected() {
    let project = ProjectIdentity::resolve(
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        Path::new("/work/ignored"),
    )
    .expect("valid project name");
    let service = ServiceIdentity::new("bbbbbbbbbbbbbbbbbbbbbbbb").expect("valid service name");

    let error = RouteIdentity::new(&project, &service).expect_err("route label must be bounded");

    assert_eq!(
        error.to_string(),
        "route label 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-bbbbbbbbbbbbbbbbbbbbbbbb' exceeds 63 bytes"
    );
}

#[test]
fn duplicate_discovery_of_the_same_canonical_path_is_deduplicated() {
    let first = route_claim("/work/bill", "bill", "app");
    let duplicate = route_claim("/work/bill", "bill", "app");

    let registry = validate_route_claims(vec![first, duplicate]).expect("valid registry");

    assert_eq!(registry.claims().len(), 1);
}

#[test]
fn distinct_paths_claiming_the_same_domain_fail_loudly() {
    let first = route_claim("/work/bill", "bill", "app");
    let second = route_claim("/work/archive/bill", "bill", "app");

    let error = validate_route_claims(vec![first, second]).expect_err("route collision");

    assert_eq!(
        error.to_string(),
        concat!(
            "route registry contains conflicting ownership:\n",
            "- bill-app.stackctl.localhost is claimed by:\n",
            "  - project 'bill', service 'app', path '/work/archive/bill'\n",
            "  - project 'bill', service 'app', path '/work/bill'",
        )
    );
}

#[test]
fn composite_name_collision_fails_instead_of_receiving_a_fallback_domain() {
    let first = route_claim("/work/bill", "bill", "app-admin");
    let second = route_claim("/work/bill-app", "bill-app", "admin");

    let error = validate_route_claims(vec![first, second]).expect_err("route collision");

    assert_eq!(error.conflicts().len(), 1);
    assert_eq!(
        error.conflicts()[0].domain(),
        "bill-app-admin.stackctl.localhost"
    );
}

#[test]
fn every_current_preset_has_one_explicit_safe_deployment_strategy() {
    let cases = [
        ("mongodb", ServiceDeploymentStrategy::SharedByCompatibility),
        ("postgres", ServiceDeploymentStrategy::SharedByCompatibility),
        ("pg", ServiceDeploymentStrategy::SharedByCompatibility),
        ("pgsql", ServiceDeploymentStrategy::SharedByCompatibility),
        ("mysql", ServiceDeploymentStrategy::SharedByCompatibility),
        ("mariadb", ServiceDeploymentStrategy::SharedByCompatibility),
        (
            "sqlserver",
            ServiceDeploymentStrategy::SharedByCompatibility,
        ),
        ("mssql", ServiceDeploymentStrategy::SharedByCompatibility),
        ("redis", ServiceDeploymentStrategy::SharedByCompatibility),
        ("valkey", ServiceDeploymentStrategy::SharedByCompatibility),
        (
            "dragonfly",
            ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        ),
        ("memcached", ServiceDeploymentStrategy::DedicatedProject),
        ("minio", ServiceDeploymentStrategy::SharedByCompatibility),
        (
            "garage",
            ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        ),
        ("localstack", ServiceDeploymentStrategy::DedicatedProject),
        (
            "rustfs",
            ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        ),
        (
            "opensearch",
            ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        ),
        (
            "elasticsearch",
            ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        ),
        (
            "meilisearch",
            ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        ),
        (
            "typesense",
            ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        ),
        ("frankenphp", ServiceDeploymentStrategy::ProjectApplication),
        ("laravel", ServiceDeploymentStrategy::ProjectApplication),
        ("reverb", ServiceDeploymentStrategy::ProjectApplication),
        ("horizon", ServiceDeploymentStrategy::ProjectProcess),
        ("queue-worker", ServiceDeploymentStrategy::ProjectProcess),
        ("queue", ServiceDeploymentStrategy::ProjectProcess),
        ("scheduler", ServiceDeploymentStrategy::ProjectProcess),
        ("dusk", ServiceDeploymentStrategy::Ephemeral),
        ("selenium", ServiceDeploymentStrategy::Ephemeral),
        ("gotenberg", ServiceDeploymentStrategy::SharedStateless),
        ("mailpit", ServiceDeploymentStrategy::SharedWithAttribution),
        ("rabbitmq", ServiceDeploymentStrategy::SharedByCompatibility),
    ];

    assert_eq!(
        cases.map(|(preset, _)| preset),
        KNOWN_SERVICE_PRESETS,
        "the strategy matrix and editor preset catalog must move together"
    );

    for (preset, expected) in cases {
        assert_eq!(
            resolve_service_deployment_strategy(preset).expect("known preset"),
            expected,
            "preset {preset}"
        );
    }
    assert_eq!(
        resolve_service_deployment_strategy("invented")
            .expect_err("unknown preset")
            .to_string(),
        "unknown v8 service preset 'invented'"
    );
    for removed in ["mailhog", "soketi"] {
        assert_eq!(
            resolve_service_deployment_strategy(removed)
                .expect_err("removed preset")
                .to_string(),
            format!("unknown v8 service preset '{removed}'")
        );
    }
}

#[test]
fn every_non_process_preset_has_a_versioned_artifact_catalog_entry() {
    assert_eq!(PRESET_ARTIFACT_CATALOG_REVISION, "2026-07-14.2");

    for preset in KNOWN_SERVICE_PRESETS {
        let strategy = resolve_service_deployment_strategy(preset).expect("known strategy");
        let artifact = resolve_preset_artifact(preset, None).expect("known artifact policy");

        if strategy == ServiceDeploymentStrategy::ProjectProcess {
            assert_eq!(artifact, None, "process preset {preset} inherits app image");
        } else {
            let artifact = artifact.unwrap_or_else(|| panic!("preset {preset} needs an artifact"));
            assert!(!artifact.version().is_empty(), "preset {preset}");
            assert!(artifact.reference().contains(':'), "preset {preset}");
        }
    }

    assert_eq!(
        resolve_preset_artifact("postgres", Some("17"))
            .expect("PostgreSQL 17")
            .expect("PostgreSQL artifact")
            .reference(),
        "postgres:17"
    );
    assert_eq!(
        resolve_preset_artifact("pg", Some("18"))
            .expect("PostgreSQL alias")
            .expect("PostgreSQL artifact")
            .reference(),
        "postgres:18"
    );
    for removed in ["mailhog", "soketi"] {
        assert!(
            resolve_preset_artifact(removed, None)
                .expect_err("removed preset")
                .to_string()
                .contains("has no built-in artifact catalog entry")
        );
    }
}

#[test]
fn preset_artifact_catalog_never_resolves_a_latest_alias() {
    for preset in KNOWN_SERVICE_PRESETS {
        let Some(artifact) = resolve_preset_artifact(preset, None).expect("known artifact policy")
        else {
            continue;
        };

        let tag = artifact
            .reference()
            .rsplit_once(':')
            .map(|(_, tag)| tag)
            .expect("catalog reference has a tag");
        assert!(
            tag != "latest" && !tag.ends_with("-latest"),
            "preset {preset} resolves mutable alias {}",
            artifact.reference()
        );
    }
}

#[test]
fn built_in_php_runtimes_use_the_stackctl_owned_extension_image() {
    for preset in ["laravel", "frankenphp", "reverb"] {
        let artifact = resolve_preset_artifact(preset, Some("8.5"))
            .expect("known PHP runtime")
            .expect("PHP runtime artifact");

        assert_eq!(artifact.reference(), "ghcr.io/faustbrian/stackctl-php:8.5");
    }
}

fn route_claim(path: &str, project: &str, service: &str) -> RouteClaim {
    let project =
        ProjectIdentity::resolve(Some(project), Path::new(path)).expect("valid project identity");
    let service = ServiceIdentity::new(service).expect("valid service identity");

    RouteClaim::new(PathBuf::from(path), project, service).expect("valid route claim")
}
