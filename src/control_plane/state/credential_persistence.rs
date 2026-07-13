use super::{CredentialLifecycle, CredentialRecord, CredentialRecordOptions, StateStoreError};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub(super) fn load_credential(
    connection: &Connection,
    credential_id: &str,
) -> Result<Option<CredentialRecord>, StateStoreError> {
    let persisted = connection
        .query_row(
            "SELECT credential_id, project_id, service_id, username, secret, lifecycle
             FROM credentials WHERE credential_id = ?1",
            [credential_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;

    persisted.map(credential_from_persisted).transpose()
}

pub(super) fn credential_from_persisted(
    persisted: (String, Option<String>, String, String, String, String),
) -> Result<CredentialRecord, StateStoreError> {
    let (credential_id, project_id, service_id, username, secret, lifecycle) = persisted;
    let lifecycle = CredentialLifecycle::from_label(&lifecycle).ok_or_else(|| {
        StateStoreError::CorruptState {
            detail: format!("credential '{credential_id}' has unknown lifecycle '{lifecycle}'"),
        }
    })?;

    Ok(CredentialRecord::new(CredentialRecordOptions {
        credential_id,
        project_id,
        service_id,
        username,
        secret,
        lifecycle,
    }))
}

/// Inserts stable credential state inside the caller's transaction.
pub(super) fn persist_credential_if_absent(
    transaction: &Transaction<'_>,
    credential: &CredentialRecord,
) -> Result<CredentialRecord, StateStoreError> {
    if let Some(existing) = load_credential(transaction, credential.credential_id())? {
        if existing.project_id() != credential.project_id()
            || existing.service_id() != credential.service_id()
            || existing.username() != credential.username()
        {
            return Err(StateStoreError::CredentialOwnershipConflict {
                credential_id: credential.credential_id().to_owned(),
            });
        }

        return Ok(existing);
    }

    transaction.execute(
        "INSERT INTO credentials (
             credential_id, project_id, service_id, username, secret, lifecycle
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            credential.credential_id(),
            credential.project_id(),
            credential.service_id(),
            credential.username(),
            credential.secret(),
            credential.lifecycle().label(),
        ],
    )?;

    Ok(credential.clone())
}
