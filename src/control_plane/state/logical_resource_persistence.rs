use super::{LogicalResourceRecord, ResourceLifecycle, StateStoreError};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub(super) struct PersistedLogicalResourceOwnership {
    shared_resource_id: String,
    project_id: String,
    service_id: String,
    kind: String,
    compatibility_fingerprint: String,
    lifecycle: String,
}

impl PersistedLogicalResourceOwnership {
    pub(super) fn matches(&self, resource: &LogicalResourceRecord) -> bool {
        self.shared_resource_id == resource.shared_resource_id()
            && self.project_id == resource.project_id()
            && self.service_id == resource.service_id()
            && self.kind == resource.kind()
            && self.compatibility_fingerprint == resource.compatibility_fingerprint()
    }

    pub(super) fn is_active(&self) -> bool {
        self.lifecycle == ResourceLifecycle::Active.label()
    }
}

pub(super) fn load_logical_resource_ownership(
    connection: &Connection,
    logical_resource_id: &str,
) -> Result<Option<PersistedLogicalResourceOwnership>, StateStoreError> {
    connection
        .query_row(
            "SELECT shared_resource_id, project_id, service_id, kind,
                    compatibility_fingerprint, lifecycle
             FROM logical_resources WHERE logical_resource_id = ?1",
            [logical_resource_id],
            |row| {
                Ok(PersistedLogicalResourceOwnership {
                    shared_resource_id: row.get(0)?,
                    project_id: row.get(1)?,
                    service_id: row.get(2)?,
                    kind: row.get(3)?,
                    compatibility_fingerprint: row.get(4)?,
                    lifecycle: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

/// Upserts exact logical ownership inside the caller's transaction.
pub(super) fn persist_logical_resources(
    transaction: &Transaction<'_>,
    resources: &[LogicalResourceRecord],
) -> Result<(), StateStoreError> {
    for resource in resources {
        if let Some(existing) =
            load_logical_resource_ownership(transaction, resource.logical_resource_id())?
        {
            if !existing.matches(resource) {
                return Err(StateStoreError::LogicalResourceOwnershipConflict {
                    logical_resource_id: resource.logical_resource_id().to_owned(),
                });
            }
            if !existing.is_active() && resource.lifecycle() == ResourceLifecycle::Active {
                return Err(StateStoreError::ProjectAdoptionRequired {
                    project_id: resource.project_id().to_owned(),
                });
            }
        }
        transaction.execute(
            "INSERT INTO logical_resources (
                 logical_resource_id, shared_resource_id, project_id, service_id,
                 kind, compatibility_fingerprint, desired_revision, lifecycle,
                 orphaned_at_unix_seconds
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(logical_resource_id) DO UPDATE SET
                 desired_revision = excluded.desired_revision,
                 lifecycle = excluded.lifecycle,
                 orphaned_at_unix_seconds = excluded.orphaned_at_unix_seconds",
            params![
                resource.logical_resource_id(),
                resource.shared_resource_id(),
                resource.project_id(),
                resource.service_id(),
                resource.kind(),
                resource.compatibility_fingerprint(),
                resource.desired_revision(),
                resource.lifecycle().label(),
                resource.orphaned_at_unix_seconds(),
            ],
        )?;
    }

    Ok(())
}
