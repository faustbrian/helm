use super::{DeletionDecision, PruneAuthorization, evaluate_deletion};
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
            PruneAuthorization::Explicit {
                verified_backup: false,
            },
        ),
        DeletionDecision::AwaitVerifiedBackup
    );
    assert_eq!(
        evaluate_deletion(
            &resource,
            50_000,
            100,
            PruneAuthorization::Explicit {
                verified_backup: true,
            },
        ),
        DeletionDecision::DeleteAuthorized
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
