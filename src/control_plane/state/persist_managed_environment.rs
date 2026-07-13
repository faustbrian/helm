use super::{EnvironmentLifecycle, ManagedEnvironmentRecord, StateStoreError};
use rusqlite::{OptionalExtension, Transaction, params};

/// Replaces one complete managed environment inside the caller's transaction.
pub(super) fn persist_managed_environment(
    transaction: &Transaction<'_>,
    environment: &ManagedEnvironmentRecord,
) -> Result<(), StateStoreError> {
    let values_json = serde_json::to_string(environment.values()).map_err(|error| {
        StateStoreError::CorruptState {
            detail: format!("failed to encode managed environment: {error}"),
        }
    })?;
    let existing_lifecycle = transaction
        .query_row(
            "SELECT lifecycle FROM managed_environments WHERE project_id = ?1",
            [environment.project_id()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if existing_lifecycle.as_deref() == Some(EnvironmentLifecycle::Disabled.label())
        && environment.lifecycle() == EnvironmentLifecycle::Active
    {
        return Err(StateStoreError::ProjectAdoptionRequired {
            project_id: environment.project_id().to_owned(),
        });
    }
    transaction.execute(
        "INSERT INTO managed_environments (
             project_id, revision, values_json, lifecycle
         ) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(project_id) DO UPDATE SET
             revision = excluded.revision,
             values_json = excluded.values_json,
             lifecycle = excluded.lifecycle",
        params![
            environment.project_id(),
            environment.revision(),
            values_json,
            environment.lifecycle().label(),
        ],
    )?;

    Ok(())
}
