use super::sqlite_state_store::{
    load_v7_migration_execution, validate_v7_migration_execution_transition,
};
use super::{StateStoreError, V7MigrationExecutionRecord};
use rusqlite::{OptionalExtension, Transaction, params};

/// Persists one monotonic v7 execution inside the caller's transaction.
pub(super) fn persist_v7_migration_execution(
    transaction: &Transaction<'_>,
    canonical_path: &str,
    execution: &V7MigrationExecutionRecord,
) -> Result<(), StateStoreError> {
    let checkpoints_json = execution.checkpoints_json().map_err(|detail| {
        StateStoreError::InvalidV7MigrationExecution {
            path: execution.canonical_project_path().to_path_buf(),
            detail,
        }
    })?;
    let accepted_project = transaction
        .query_row(
            "SELECT project_id FROM accepted_v7_inventories
             WHERE canonical_project_path = ?1 AND evidence_revision = ?2",
            params![canonical_path, execution.evidence_revision()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if accepted_project.as_deref() != Some(execution.project_id()) {
        return Err(StateStoreError::InvalidV7MigrationExecution {
            path: execution.canonical_project_path().to_path_buf(),
            detail: "has no matching accepted v7 source evidence".to_owned(),
        });
    }
    if let Some(previous) =
        load_v7_migration_execution(transaction, canonical_path, execution.evidence_revision())?
    {
        validate_v7_migration_execution_transition(&previous, execution)?;
    }
    transaction.execute(
        "INSERT INTO v7_migration_executions (
             canonical_project_path, project_id, evidence_revision,
             adapter_plan_revision, phase, checkpoints_json,
             updated_at_unix_seconds
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(canonical_project_path, evidence_revision) DO UPDATE SET
             phase = excluded.phase,
             checkpoints_json = excluded.checkpoints_json,
             updated_at_unix_seconds = excluded.updated_at_unix_seconds",
        params![
            canonical_path,
            execution.project_id(),
            execution.evidence_revision(),
            execution.adapter_plan_revision(),
            execution.phase().label(),
            checkpoints_json,
            execution.updated_at_unix_seconds(),
        ],
    )?;

    Ok(())
}
