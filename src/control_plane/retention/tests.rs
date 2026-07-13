use super::{
    BackupArtifactManifest, DeletionDecision, PruneAuthorization, evaluate_deletion,
    verify_backup_artifact,
};
use crate::control_plane::state::{
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
};

#[test]
fn active_resources_are_never_garbage_collected() {
    let resource = resource(
        ResourceRetention::Disposable,
        ResourceLifecycle::Active,
        None,
    );

    assert_eq!(
        evaluate_deletion(&resource, 10_000, 100, PruneAuthorization::None),
        DeletionDecision::KeepActive
    );
}

#[test]
fn expired_disposable_orphans_are_automatically_deletable() {
    let resource = resource(
        ResourceRetention::Disposable,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    assert_eq!(
        evaluate_deletion(&resource, 1_101, 100, PruneAuthorization::None),
        DeletionDecision::DeleteDisposable
    );
}

#[test]
fn unexpired_disposable_orphans_are_retained() {
    let resource = resource(
        ResourceRetention::Disposable,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    assert_eq!(
        evaluate_deletion(&resource, 1_099, 100, PruneAuthorization::None),
        DeletionDecision::StopAndRetain
    );
}

#[test]
fn build_caches_follow_the_disposable_retention_window() {
    let resource = resource(
        ResourceRetention::BuildCache,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    assert_eq!(
        evaluate_deletion(&resource, 1_100, 100, PruneAuthorization::None),
        DeletionDecision::DeleteDisposable
    );
}

#[test]
fn disposable_resources_without_an_orphan_timestamp_are_retained() {
    let resource = resource(
        ResourceRetention::Disposable,
        ResourceLifecycle::Retained,
        None,
    );

    assert_eq!(
        evaluate_deletion(&resource, 50_000, 100, PruneAuthorization::None),
        DeletionDecision::StopAndRetain
    );
}

#[test]
fn persistent_orphans_require_explicit_prune_and_verified_backup() {
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    assert_eq!(
        evaluate_deletion(&resource, 50_000, 100, PruneAuthorization::None),
        DeletionDecision::StopAndRetain
    );
    assert_eq!(
        evaluate_deletion(
            &resource,
            50_000,
            100,
            PruneAuthorization::Explicit { backup: None },
        ),
        DeletionDecision::AwaitVerifiedBackup
    );
    let manifest = BackupArtifactManifest::from_artifact(&resource, b"verified backup", 49_000)
        .expect("backup manifest");
    let backup = verify_backup_artifact(&manifest, b"verified backup", 49_500)
        .expect("verified backup evidence");
    assert_eq!(
        evaluate_deletion(
            &resource,
            50_000,
            100,
            PruneAuthorization::Explicit {
                backup: Some(backup),
            },
        ),
        DeletionDecision::DeleteAuthorized
    );
}

#[test]
fn persistent_prune_rejects_tampered_or_wrong_resource_backup_evidence() {
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let manifest = BackupArtifactManifest::from_artifact(&resource, b"backup bytes", 40_000)
        .expect("backup manifest");

    let error = verify_backup_artifact(&manifest, b"tampered bytes", 41_000)
        .expect_err("tampered artifact");
    assert_eq!(
        error.to_string(),
        "backup artifact checksum does not match its manifest"
    );

    let other = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "resource-2".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:other-fingerprint".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(1_000),
    });
    let evidence = verify_backup_artifact(&manifest, b"backup bytes", 41_000)
        .expect("valid but wrong evidence");
    assert_eq!(
        evaluate_deletion(
            &other,
            50_000,
            100,
            PruneAuthorization::Explicit {
                backup: Some(evidence),
            },
        ),
        DeletionDecision::AwaitVerifiedBackup
    );
}

#[test]
fn backup_manifest_serializes_portable_resource_identity_and_checksum() {
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let manifest = BackupArtifactManifest::from_artifact(&resource, b"backup bytes", 40_000)
        .expect("backup manifest");

    assert_eq!(
        serde_json::to_value(manifest).expect("manifest JSON"),
        serde_json::json!({
            "schema_version": 1,
            "resource_id": "resource-1",
            "installation_id": "install-1",
            "compatibility_fingerprint": "sha256:fingerprint",
            "artifact_sha256":
                "7171b7ccbaa1ac3767c1e75815c6c5bca6634f141b55b4d1a398ddf2a76b75df",
            "created_at_unix_seconds": 40_000,
        })
    );
}

#[test]
fn backup_verification_rejects_impossible_timestamps() {
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let manifest = BackupArtifactManifest::from_artifact(&resource, b"backup bytes", 40_000)
        .expect("backup manifest");

    let error = verify_backup_artifact(&manifest, b"backup bytes", 39_999)
        .expect_err("verification before creation");
    assert_eq!(
        error.to_string(),
        "backup verification time predates artifact creation"
    );
}

fn resource(
    retention: ResourceRetention,
    lifecycle: ResourceLifecycle,
    orphaned_at_unix_seconds: Option<i64>,
) -> ResourceRecord {
    ResourceRecord::new(ResourceRecordOptions {
        resource_id: "resource-1".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:fingerprint".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention,
        lifecycle,
        orphaned_at_unix_seconds,
    })
}
