use super::{
    BackupArtifactManifest, DeletionDecision, PruneAuthorization, evaluate_deletion,
    store_backup_artifact, verify_backup_artifact, verify_stored_backup_artifact,
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
            "resource_kind": "volume",
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

#[cfg(unix)]
#[test]
fn backup_artifacts_are_private_atomic_immutable_and_reread_for_verification() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-store-{}-{}",
        std::process::id(),
        50_000
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    let stored = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("stored backup");
    let repeated = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("idempotent backup store");

    assert_eq!(stored, repeated);
    assert_eq!(
        std::fs::read(stored.artifact_file()).expect("artifact bytes"),
        b"recoverable bytes"
    );
    assert_eq!(
        std::fs::metadata(&root)
            .expect("backup root metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for path in [stored.artifact_file(), stored.manifest_file()] {
        assert_eq!(
            std::fs::metadata(path)
                .expect("backup file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    let evidence = verify_stored_backup_artifact(&stored, 41_000).expect("stored evidence");
    assert_eq!(
        evaluate_deletion(
            &resource,
            50_000,
            100,
            PruneAuthorization::Explicit {
                backup: Some(evidence),
            },
        ),
        DeletionDecision::DeleteAuthorized
    );

    std::fs::write(stored.artifact_file(), b"tampered").expect("tamper artifact");
    let error = verify_stored_backup_artifact(&stored, 41_000).expect_err("tampered stored backup");
    assert_eq!(
        error.to_string(),
        "backup artifact checksum does not match its manifest"
    );

    std::fs::remove_dir_all(&root).expect("remove backup fixture");
}

#[cfg(unix)]
#[test]
fn backup_store_retains_distinct_recovery_points_for_one_resource() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-history-{}-{}",
        std::process::id(),
        50_001
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    let first =
        store_backup_artifact(&resource, b"first backup", 40_000, &root).expect("first backup");
    let second =
        store_backup_artifact(&resource, b"second backup", 41_000, &root).expect("second backup");

    assert_ne!(first, second);
    assert!(first.artifact_file().is_file());
    assert!(second.artifact_file().is_file());

    std::fs::remove_dir_all(&root).expect("remove backup history fixture");
}

#[cfg(unix)]
#[test]
fn backup_store_recovers_an_incomplete_owned_pending_publish() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-recovery-{}-{}",
        std::process::id(),
        50_002
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let initially_stored = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("initial backup");
    let destination = initially_stored
        .artifact_file()
        .parent()
        .expect("destination directory")
        .to_owned();
    let resource_directory = destination.parent().expect("resource directory");
    let pending = resource_directory.join(format!(
        ".pending-{}",
        destination
            .file_name()
            .expect("destination file name")
            .to_string_lossy()
    ));
    std::fs::remove_dir_all(&destination).expect("simulate unpublished backup");
    std::fs::create_dir(&pending).expect("stale pending directory");
    std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o700))
        .expect("protect pending directory");
    std::fs::write(pending.join("artifact.bin"), b"partial").expect("partial pending artifact");

    let recovered = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("recovered backup publish");

    assert_eq!(recovered, initially_stored);
    assert!(!pending.exists());
    verify_stored_backup_artifact(&recovered, 41_000).expect("verified recovered backup");

    std::fs::remove_dir_all(&root).expect("remove backup recovery fixture");
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
