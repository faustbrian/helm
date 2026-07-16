use super::{LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle};

pub(crate) fn staged_migration_target(
    target: &LogicalResourceRecord,
    migration_id: &str,
) -> LogicalResourceRecord {
    copy_with_identity(
        target,
        staged_target_id(target.project_id(), target.service_id(), migration_id),
        ResourceLifecycle::Retained,
    )
}

pub(crate) fn retained_migration_source(
    source: &LogicalResourceRecord,
    migration_id: &str,
) -> LogicalResourceRecord {
    copy_with_identity(
        source,
        retained_source_id(source.project_id(), source.service_id(), migration_id),
        ResourceLifecycle::Retained,
    )
}

pub(crate) fn active_migration_resource(resource: &LogicalResourceRecord) -> LogicalResourceRecord {
    copy_with_identity(
        resource,
        canonical_logical_resource_id(resource.project_id(), resource.service_id()),
        ResourceLifecycle::Active,
    )
}

pub(crate) fn staged_target_id(project_id: &str, service_id: &str, migration_id: &str) -> String {
    format!(
        "{}#migration:{migration_id}:target",
        canonical_logical_resource_id(project_id, service_id)
    )
}

pub(crate) fn retained_source_id(project_id: &str, service_id: &str, migration_id: &str) -> String {
    format!(
        "{}#migration:{migration_id}:source",
        canonical_logical_resource_id(project_id, service_id)
    )
}

fn canonical_logical_resource_id(project_id: &str, service_id: &str) -> String {
    format!("{project_id}/{service_id}")
}

fn copy_with_identity(
    resource: &LogicalResourceRecord,
    logical_resource_id: String,
    lifecycle: ResourceLifecycle,
) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id,
        shared_resource_id: resource.shared_resource_id().to_owned(),
        project_id: resource.project_id().to_owned(),
        service_id: resource.service_id().to_owned(),
        kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        desired_revision: resource.desired_revision().to_owned(),
        lifecycle,
        orphaned_at_unix_seconds: resource.orphaned_at_unix_seconds(),
    })
}
