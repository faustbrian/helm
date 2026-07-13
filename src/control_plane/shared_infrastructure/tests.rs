use super::{
    CompatibilityFingerprint, CompatibilityFingerprintOptions, CredentialEntropy,
    CredentialGenerationError, CredentialSecret, IsolationCapability, PersistenceMode,
    PostgresLogicalResourcePlan, SharedServiceRequest, generate_credential_secret,
    plan_shared_instances,
};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn equivalent_postgres_profiles_share_one_fingerprint() {
    let first = fingerprint(vec!["postgis", "pg_stat_statements"], "17");
    let second = fingerprint(vec!["pg_stat_statements", "postgis"], "17");

    assert_eq!(first, second);
    assert_eq!(first.as_str().len(), 71);
    assert!(first.as_str().starts_with("sha256:"));
}

#[test]
fn different_major_versions_never_share_a_fingerprint() {
    let postgres_17 = fingerprint(vec!["postgis"], "17");
    let postgres_18 = fingerprint(vec!["postgis"], "18");

    assert_ne!(postgres_17, postgres_18);
}

#[test]
fn immutable_settings_participate_in_the_fingerprint() {
    let mut options = postgres_options(vec!["postgis"], "17");
    let baseline =
        CompatibilityFingerprint::from_options(options.clone()).expect("baseline fingerprint");
    options
        .immutable_settings
        .insert("locale".to_owned(), "fi_FI.UTF-8".to_owned());

    let localized = CompatibilityFingerprint::from_options(options).expect("localized fingerprint");

    assert_ne!(baseline, localized);
}

#[test]
fn incomplete_compatibility_profiles_fail_before_planning() {
    let mut options = postgres_options(Vec::new(), "17");
    options.implementation.clear();

    let error = CompatibilityFingerprint::from_options(options)
        .expect_err("missing implementation identity");

    assert_eq!(
        error.to_string(),
        "compatibility implementation must not be empty"
    );
}

#[test]
fn mutable_image_references_cannot_identify_shared_instances() {
    let mut options = postgres_options(Vec::new(), "17");
    options.image_digest = "postgres:17".to_owned();

    let error =
        CompatibilityFingerprint::from_options(options).expect_err("mutable image reference");

    assert_eq!(
        error.to_string(),
        "compatibility image 'postgres:17' must use an immutable sha256 digest"
    );
}

#[test]
fn forty_projects_across_two_postgres_majors_plan_two_instances() {
    let requests = (0..40)
        .map(|index| {
            let major = if index < 20 { "17" } else { "18" };

            SharedServiceRequest::new(
                format!("project-{index:02}"),
                "database",
                fingerprint(vec!["postgis"], major),
            )
        })
        .collect();

    let plans = plan_shared_instances(requests);

    assert_eq!(plans.len(), 2);
    assert_eq!(
        plans
            .iter()
            .map(|plan| plan.consumers().len())
            .collect::<Vec<_>>(),
        vec![20, 20]
    );
    assert_eq!(
        plans
            .iter()
            .map(|plan| plan.fingerprint())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
}

#[test]
fn repeated_identical_consumers_do_not_duplicate_logical_ownership() {
    let request = SharedServiceRequest::new("bill", "database", fingerprint(Vec::new(), "17"));

    let plans = plan_shared_instances(vec![request.clone(), request]);
    let consumers = plans
        .first()
        .map(|plan| plan.consumers())
        .unwrap_or_default();

    assert_eq!(plans.len(), 1);
    assert_eq!(consumers.len(), 1);
    assert_eq!(
        consumers.first().map(|owner| owner.project_id()),
        Some("bill")
    );
    assert_eq!(
        consumers.first().map(|owner| owner.service_id()),
        Some("database")
    );
}

#[test]
fn managed_credentials_use_256_bits_of_injected_entropy_and_redact_debug() {
    let secret = generate_credential_secret(&SequentialEntropy).expect("managed secret");

    assert_eq!(
        secret.expose(),
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
    );
    assert_eq!(format!("{secret:?}"), "CredentialSecret([REDACTED])");
}

#[test]
fn postgres_logical_resources_use_deterministic_isolated_names_and_stdin() {
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL logical plan");

    assert_eq!(plan.database_name(), "stackctl_bill_database");
    assert_eq!(plan.role_name(), "stackctl_bill_database_role");
    assert_eq!(plan.credential_id(), "bill/database/postgresql");
    assert_eq!(
        plan.command_arguments(),
        [
            "psql",
            "--no-psqlrc",
            "--set=ON_ERROR_STOP=1",
            "--username=postgres",
            "--dbname=postgres",
        ]
    );
    assert!(plan.stdin_sql().contains("CREATE DATABASE"));
    assert!(plan.stdin_sql().contains("REVOKE ALL"));
    assert!(plan.stdin_sql().contains("project-secret"));
    assert!(!format!("{plan:?}").contains("project-secret"));
}

#[test]
fn postgres_logical_resources_fail_instead_of_shortening_identifiers() {
    let project_id = "a".repeat(50);

    let error = PostgresLogicalResourcePlan::new(
        &project_id,
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect_err("overlong PostgreSQL role");

    assert!(
        error
            .to_string()
            .contains("exceeds PostgreSQL's 63-byte limit")
    );
}

struct SequentialEntropy;

impl CredentialEntropy for SequentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index).expect("test entropy index");
        }

        Ok(())
    }
}

fn fingerprint(extensions: Vec<&str>, major_version: &str) -> CompatibilityFingerprint {
    CompatibilityFingerprint::from_options(postgres_options(extensions, major_version))
        .expect("valid compatibility fingerprint")
}

fn postgres_options(extensions: Vec<&str>, major_version: &str) -> CompatibilityFingerprintOptions {
    CompatibilityFingerprintOptions {
        implementation: "postgresql".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        extensions: extensions.into_iter().map(str::to_owned).collect(),
        immutable_settings: BTreeMap::from([(
            "authentication".to_owned(),
            "scram-sha-256".to_owned(),
        )]),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some("linux/arm64".to_owned()),
    }
}
