use super::StateStoreError;
use rusqlite::{Connection, TransactionBehavior};

pub(super) const MINIMUM_MIGRATABLE_SCHEMA_VERSION: u32 = 16;

pub(super) fn migrate_sqlite_state_schema(
    connection: &mut Connection,
    found: u32,
    current: u32,
) -> Result<(), StateStoreError> {
    if found != MINIMUM_MIGRATABLE_SCHEMA_VERSION || current != 17 {
        return Err(StateStoreError::UnsupportedSchema {
            found,
            supported: current,
        });
    }

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "ALTER TABLE daemon_operations
             ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0
             CHECK(retry_count >= 0);",
    )?;
    transaction.pragma_update(None, "user_version", current)?;
    transaction.commit()?;

    Ok(())
}
