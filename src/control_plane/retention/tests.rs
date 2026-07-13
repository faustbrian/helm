use super::{
    BackupArtifactManifest, DeletionDecision, PruneAuthorization, RestoreTarget,
    RestoreTargetError, evaluate_deletion, restore_verified_backup, store_backup_artifact,
    store_backup_artifact_from_reader, verify_backup_artifact, verify_stored_backup_artifact,
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
            "artifact_size_bytes": 12,
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
    let pending = resource_directory.join(".pending-40000");
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

#[cfg(unix)]
#[test]
fn backup_store_streams_large_artifacts_without_requiring_one_byte_buffer() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-stream-{}-{}",
        std::process::id(),
        50_003
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let bytes = vec![b'x'; 256 * 1024 + 17];
    let reader = ChunkedReader::new(&bytes, 37);

    let stored = store_backup_artifact_from_reader(&resource, reader, 42_000, &root)
        .expect("streamed backup");

    assert_eq!(
        std::fs::metadata(stored.artifact_file())
            .expect("artifact metadata")
            .len(),
        u64::try_from(bytes.len()).expect("fixture length")
    );
    verify_stored_backup_artifact(&stored, 42_001).expect("verified streamed backup");

    std::fs::remove_dir_all(&root).expect("remove streamed backup fixture");
}

#[cfg(unix)]
#[test]
fn restore_stages_verifies_and_commits_exact_verified_bytes() {
    let fixture = RestoreFixture::new("successful");
    let mut target = RecordingRestoreTarget::default();

    let evidence = restore_verified_backup(
        "restore-1",
        &fixture.resource,
        &fixture.stored,
        41_000,
        &mut target,
    )
    .expect("restored backup");

    assert_eq!(target.operations, ["stage", "verify", "commit"]);
    assert_eq!(target.staged_bytes, b"recoverable bytes");
    assert_eq!(evidence.artifact_size_bytes(), 17);
    assert!(!evidence.artifact_sha256().is_empty());

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rejects_corruption_before_mutating_the_target() {
    let fixture = RestoreFixture::new("corrupt");
    std::fs::write(fixture.stored.artifact_file(), b"tampered").expect("tamper backup");
    let mut target = RecordingRestoreTarget::default();

    let error = restore_verified_backup(
        "restore-1",
        &fixture.resource,
        &fixture.stored,
        41_000,
        &mut target,
    )
    .expect_err("corrupt backup");

    assert_eq!(
        error.to_string(),
        "backup artifact checksum does not match its manifest"
    );
    assert!(target.operations.is_empty());

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rejects_wrong_resource_evidence_before_mutating_the_target() {
    let fixture = RestoreFixture::new("wrong-resource");
    let other = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "resource-2".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:fingerprint".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(1_000),
    });
    let mut target = RecordingRestoreTarget::default();

    let error = restore_verified_backup("restore-1", &other, &fixture.stored, 41_000, &mut target)
        .expect_err("wrong resource backup");

    assert_eq!(
        error.to_string(),
        "backup evidence does not match the restore resource"
    );
    assert!(target.operations.is_empty());

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rejects_unsafe_restore_ids_before_mutating_the_target() {
    let fixture = RestoreFixture::new("unsafe-id");
    let mut target = RecordingRestoreTarget::default();

    for restore_id in ["", "../restore", "restore/child", "restore\0child"] {
        let error = restore_verified_backup(
            restore_id,
            &fixture.resource,
            &fixture.stored,
            41_000,
            &mut target,
        )
        .expect_err("unsafe restore id");

        assert_eq!(error.to_string(), "restore id must be non-empty and valid");
    }
    assert!(target.operations.is_empty());

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rolls_back_when_the_target_consumes_only_a_prefix() {
    let fixture = RestoreFixture::new("prefix");
    let mut target = RecordingRestoreTarget {
        read_limit: Some(4),
        ..RecordingRestoreTarget::default()
    };

    let error = restore_verified_backup(
        "restore-1",
        &fixture.resource,
        &fixture.stored,
        41_000,
        &mut target,
    )
    .expect_err("partial restore");

    assert_eq!(
        error.to_string(),
        "restore target did not consume the complete backup artifact"
    );
    assert_eq!(target.operations, ["stage", "rollback"]);

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rolls_back_target_verification_and_commit_failures() {
    for (failure, expected_operations) in [
        (RestoreFailure::Stage, vec!["stage", "rollback"]),
        (RestoreFailure::Verify, vec!["stage", "verify", "rollback"]),
        (
            RestoreFailure::Commit,
            vec!["stage", "verify", "commit", "rollback"],
        ),
    ] {
        let fixture = RestoreFixture::new(failure.label());
        let mut target = RecordingRestoreTarget {
            failure,
            ..RecordingRestoreTarget::default()
        };

        let error = restore_verified_backup(
            "restore-1",
            &fixture.resource,
            &fixture.stored,
            41_000,
            &mut target,
        )
        .expect_err("target failure");

        assert_eq!(
            error.to_string(),
            format!("restore target {failure} failed")
        );
        assert_eq!(target.operations, expected_operations);

        fixture.remove();
    }
}

#[cfg(unix)]
#[test]
fn restore_reports_both_primary_and_rollback_failures() {
    let fixture = RestoreFixture::new("rollback-failure");
    let mut target = RecordingRestoreTarget {
        failure: RestoreFailure::VerifyAndRollback,
        ..RecordingRestoreTarget::default()
    };

    let error = restore_verified_backup(
        "restore-1",
        &fixture.resource,
        &fixture.stored,
        41_000,
        &mut target,
    )
    .expect_err("rollback failure");

    assert_eq!(
        error.to_string(),
        "restore target verify failed; rollback also failed: restore target rollback failed"
    );
    assert_eq!(target.operations, ["stage", "verify", "rollback"]);

    fixture.remove();
}

#[cfg(unix)]
struct RestoreFixture {
    root: std::path::PathBuf,
    resource: ResourceRecord,
    stored: super::StoredBackupArtifact,
}

#[cfg(unix)]
impl RestoreFixture {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("stackctl-restore-{label}-{}", std::process::id()));
        let resource = resource(
            ResourceRetention::Persistent,
            ResourceLifecycle::Orphaned,
            Some(1_000),
        );
        let stored = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
            .expect("stored restore fixture");

        Self {
            root,
            resource,
            stored,
        }
    }

    fn remove(self) {
        std::fs::remove_dir_all(self.root).expect("remove restore fixture");
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RestoreFailure {
    #[default]
    None,
    Stage,
    Verify,
    Commit,
    VerifyAndRollback,
}

#[cfg(unix)]
impl RestoreFailure {
    const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Stage => "stage",
            Self::Verify => "verify",
            Self::Commit => "commit",
            Self::VerifyAndRollback => "verify-rollback",
        }
    }
}

#[cfg(unix)]
impl std::fmt::Display for RestoreFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

#[cfg(unix)]
#[derive(Default)]
struct RecordingRestoreTarget {
    operations: Vec<&'static str>,
    staged_bytes: Vec<u8>,
    read_limit: Option<u64>,
    failure: RestoreFailure,
}

#[cfg(unix)]
impl RestoreTarget for RecordingRestoreTarget {
    fn stage(
        &mut self,
        _restore_id: &str,
        _resource: &ResourceRecord,
        input: &mut dyn std::io::Read,
    ) -> Result<(), RestoreTargetError> {
        self.operations.push("stage");
        if self.failure == RestoreFailure::Stage {
            return Err(RestoreTargetError::new("restore target stage failed"));
        }
        if let Some(read_limit) = self.read_limit {
            let mut prefix = vec![0_u8; usize::try_from(read_limit).expect("read limit")];
            input.read_exact(&mut prefix).expect("read staged prefix");
            self.staged_bytes.extend(prefix);
        } else {
            input
                .read_to_end(&mut self.staged_bytes)
                .expect("read staged backup");
        }

        Ok(())
    }

    fn verify(
        &mut self,
        _restore_id: &str,
        _resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError> {
        self.operations.push("verify");
        if matches!(
            self.failure,
            RestoreFailure::Verify | RestoreFailure::VerifyAndRollback
        ) {
            return Err(RestoreTargetError::new("restore target verify failed"));
        }

        Ok(())
    }

    fn commit(
        &mut self,
        _restore_id: &str,
        _resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError> {
        self.operations.push("commit");
        if self.failure == RestoreFailure::Commit {
            return Err(RestoreTargetError::new("restore target commit failed"));
        }

        Ok(())
    }

    fn rollback(
        &mut self,
        _restore_id: &str,
        _resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError> {
        self.operations.push("rollback");
        if self.failure == RestoreFailure::VerifyAndRollback {
            return Err(RestoreTargetError::new("restore target rollback failed"));
        }

        Ok(())
    }
}

#[cfg(unix)]
struct ChunkedReader<'bytes> {
    bytes: &'bytes [u8],
    offset: usize,
    maximum_chunk: usize,
}

#[cfg(unix)]
impl<'bytes> ChunkedReader<'bytes> {
    const fn new(bytes: &'bytes [u8], maximum_chunk: usize) -> Self {
        Self {
            bytes,
            offset: 0,
            maximum_chunk,
        }
    }
}

#[cfg(unix)]
impl std::io::Read for ChunkedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let remaining = &self.bytes[self.offset..];
        let count = remaining.len().min(buffer.len()).min(self.maximum_chunk);
        buffer[..count].copy_from_slice(&remaining[..count]);
        self.offset += count;

        Ok(count)
    }
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
