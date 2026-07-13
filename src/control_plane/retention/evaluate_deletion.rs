use super::{DeletionDecision, PruneAuthorization};
use crate::control_plane::state::{ResourceLifecycle, ResourceRecord, ResourceRetention};

/// Determines whether a managed resource may be deleted without performing I/O.
pub(crate) fn evaluate_deletion(
    resource: &ResourceRecord,
    now_unix_seconds: i64,
    orphan_retention_seconds: i64,
    authorization: PruneAuthorization,
) -> DeletionDecision {
    if resource.lifecycle() == ResourceLifecycle::Active {
        return DeletionDecision::KeepActive;
    }

    match resource.retention() {
        ResourceRetention::Persistent => evaluate_persistent(authorization),
        ResourceRetention::Disposable | ResourceRetention::BuildCache => {
            evaluate_disposable(resource, now_unix_seconds, orphan_retention_seconds)
        }
    }
}

const fn evaluate_persistent(authorization: PruneAuthorization) -> DeletionDecision {
    match authorization {
        PruneAuthorization::None => DeletionDecision::StopAndRetain,
        PruneAuthorization::Explicit {
            verified_backup: false,
        } => DeletionDecision::AwaitVerifiedBackup,
        PruneAuthorization::Explicit {
            verified_backup: true,
        } => DeletionDecision::DeleteAuthorized,
    }
}

fn evaluate_disposable(
    resource: &ResourceRecord,
    now_unix_seconds: i64,
    orphan_retention_seconds: i64,
) -> DeletionDecision {
    let Some(orphaned_at_unix_seconds) = resource.orphaned_at_unix_seconds() else {
        return DeletionDecision::StopAndRetain;
    };
    let retention_seconds = orphan_retention_seconds.max(0);

    if now_unix_seconds.saturating_sub(orphaned_at_unix_seconds) >= retention_seconds {
        DeletionDecision::DeleteDisposable
    } else {
        DeletionDecision::StopAndRetain
    }
}
