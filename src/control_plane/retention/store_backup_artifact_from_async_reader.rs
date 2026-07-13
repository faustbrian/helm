#[cfg(unix)]
use super::BackupArtifactManifest;
#[cfg(unix)]
use super::store_backup_artifact::{
    encode_manifest, prepare_pending_backup, publish_pending_backup, storage_error,
    write_private_file,
};
use super::{BackupResourceIdentity, BackupVerificationError, StoredBackupArtifact};
#[cfg(unix)]
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncRead;
#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Streams one asynchronous input directly into an immutable recovery point.
#[cfg(unix)]
pub(crate) async fn store_backup_artifact_from_async_reader(
    resource: &BackupResourceIdentity,
    artifact: &mut (impl AsyncRead + Unpin),
    created_at_unix_seconds: i64,
    root: &Path,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    let (resource_directory, pending, pending_stored) =
        prepare_pending_backup(resource, created_at_unix_seconds, root)?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(pending_stored.artifact_file())
        .await
        .map_err(|error| {
            storage_error(
                "create private backup artifact",
                pending_stored.artifact_file(),
                error,
            )
        })?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = artifact.read(&mut buffer).await.map_err(|error| {
            storage_error(
                "read asynchronous backup input",
                pending_stored.artifact_file(),
                error,
            )
        })?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count]).await.map_err(|error| {
            storage_error(
                "write private backup artifact",
                pending_stored.artifact_file(),
                error,
            )
        })?;
        digest.update(&buffer[..count]);
        size = size.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
    if size == 0 {
        return Err(BackupVerificationError::EmptyArtifact);
    }
    file.sync_all().await.map_err(|error| {
        storage_error(
            "sync private backup artifact",
            pending_stored.artifact_file(),
            error,
        )
    })?;
    drop(file);

    let manifest = BackupArtifactManifest::from_checksum(
        resource,
        hex::encode(digest.finalize()),
        size,
        created_at_unix_seconds,
    )?;
    let manifest_bytes = encode_manifest(&manifest)?;
    write_private_file(pending_stored.manifest_file(), &manifest_bytes)?;
    std::fs::File::open(&pending)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| storage_error("sync pending backup directory", &pending, error))?;

    publish_pending_backup(
        resource,
        created_at_unix_seconds,
        &resource_directory,
        &pending,
        &manifest,
        &manifest_bytes,
    )
}

#[cfg(not(unix))]
pub(crate) async fn store_backup_artifact_from_async_reader(
    _resource: &BackupResourceIdentity,
    _artifact: &mut (impl AsyncRead + Unpin),
    _created_at_unix_seconds: i64,
    root: &Path,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    Err(BackupVerificationError::Storage {
        detail: format!(
            "secure asynchronous backup persistence is not implemented for '{}' on this platform",
            root.display()
        ),
    })
}
