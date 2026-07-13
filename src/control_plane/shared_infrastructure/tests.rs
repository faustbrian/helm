use super::{
    CompatibilityFingerprint, CompatibilityFingerprintOptions, IsolationCapability, PersistenceMode,
};
use std::collections::BTreeMap;

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
