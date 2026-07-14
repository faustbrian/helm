use super::inventory_v7_host_artifacts::read_regular;
use super::{
    V7InstallationTrustMigrationAdapterOptions, V7MigrationAdapterExecutor,
    V7MigrationAdapterTarget,
};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, store_backup_artifact_for_identity,
    verify_stored_backup_artifact,
};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;
use crate::control_plane::tls::{CertificateTrustStore, LocalCaIdentity, ensure_ca_trusted};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Preserves installation-scoped legacy trust while Stackctl CA becomes active.
pub(super) struct V7InstallationTrustMigrationAdapter<'operation> {
    trust_store: &'operation (dyn CertificateTrustStore + Sync),
    target_certificate_path: &'operation Path,
    target_identity: LocalCaIdentity,
    identity: BackupResourceIdentity,
    backup_root: &'operation Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    bundle_bytes: Vec<u8>,
}

impl<'operation> V7InstallationTrustMigrationAdapter<'operation> {
    pub(super) fn new(
        project_id: &str,
        evidence_revision: &str,
        options: V7InstallationTrustMigrationAdapterOptions<'operation>,
    ) -> Result<Self, String> {
        validate_accepted_artifacts(&options)?;
        if !options.backup_root.is_absolute()
            || options.created_at_unix_seconds < 0
            || options.verified_at_unix_seconds < options.created_at_unix_seconds
            || options.legacy_ca_artifacts.is_empty()
        {
            return Err(
                "v7 trust adapter requires accepted CAs, an absolute backup root, and non-regressing times"
                    .to_owned(),
            );
        }
        let target_pem = read_certificate(options.target_certificate_path)?;
        let target_identity = LocalCaIdentity::from_pem(&target_pem)
            .map_err(|error| format!("Stackctl target CA is invalid: {error}"))?;
        let mut certificates = Vec::with_capacity(options.legacy_ca_artifacts.len());
        for artifact in options.legacy_ca_artifacts {
            let maximum_bytes = usize::try_from(artifact.size_bytes())
                .map_err(|_| "legacy CA size exceeds filesystem limits".to_owned())?;
            let observed = read_regular(artifact.path(), maximum_bytes, true)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "accepted legacy CA disappeared".to_owned())?;
            let revision = format!("sha256:{}", hex::encode(Sha256::digest(&observed.bytes)));
            if observed.size_bytes != artifact.size_bytes() || revision != artifact.revision() {
                return Err(format!(
                    "legacy CA '{}' differs from accepted evidence",
                    artifact.path().display()
                ));
            }
            let pem = std::str::from_utf8(&observed.bytes)
                .map_err(|_| "legacy CA is not valid UTF-8 PEM".to_owned())?;
            LocalCaIdentity::from_pem(pem)
                .map_err(|error| format!("legacy CA is invalid: {error}"))?;
            certificates.push(STANDARD.encode(&observed.bytes));
        }
        let bundle_bytes = serde_json::to_vec(&LegacyTrustBundle {
            schema_version: 1,
            certificates_base64: certificates,
        })
        .map_err(|error| format!("failed to encode legacy trust backup: {error}"))?;

        Ok(Self {
            trust_store: options.trust_store,
            target_certificate_path: options.target_certificate_path,
            target_identity,
            identity: BackupResourceIdentity::for_v7_legacy_trust(project_id, evidence_revision),
            backup_root: options.backup_root,
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            bundle_bytes,
        })
    }

    fn verified_bundle(
        &self,
        checkpoint: &V7MigrationAdapterCheckpoint,
    ) -> Result<(PathBuf, LegacyTrustBundle), MigrationOperationError> {
        let reference = checkpoint.recovery_reference().ok_or_else(|| {
            MigrationOperationError::new("legacy trust checkpoint has no recovery reference")
        })?;
        let stored = open_stored_backup_artifact(reference)
            .map_err(|error| operation_error("open legacy trust backup", error))?;
        let verified = verify_stored_backup_artifact(&stored, self.verified_at_unix_seconds)
            .map_err(|error| operation_error("verify legacy trust backup", error))?;
        let bytes = std::fs::read(stored.artifact_file())
            .map_err(|error| operation_error("read legacy trust backup", error))?;
        if !verified.matches_identity(&self.identity)
            || checkpoint.recovery_artifact_sha256() != Some(verified.artifact_sha256())
            || checkpoint.recovery_artifact_size_bytes() != Some(verified.artifact_size_bytes())
            || bytes != self.bundle_bytes
        {
            return Err(MigrationOperationError::new(
                "legacy trust backup does not match accepted evidence",
            ));
        }
        let bundle = serde_json::from_slice::<LegacyTrustBundle>(&bytes)
            .map_err(|error| operation_error("decode legacy trust backup", error))?;
        if bundle.schema_version != 1 || bundle.certificates_base64.is_empty() {
            return Err(MigrationOperationError::new(
                "legacy trust backup has an unsupported envelope",
            ));
        }

        Ok((stored.recovery_point().to_path_buf(), bundle))
    }
}

impl V7MigrationAdapterExecutor for V7InstallationTrustMigrationAdapter<'_> {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        Box::pin(async move {
            let stored = store_backup_artifact_for_identity(
                &self.identity,
                &self.bundle_bytes,
                self.created_at_unix_seconds,
                self.backup_root,
            )
            .map_err(|error| operation_error("store legacy trust backup", error))?;
            let verified = verify_stored_backup_artifact(&stored, self.verified_at_unix_seconds)
                .map_err(|error| operation_error("verify legacy trust backup", error))?;
            let reference = stored.recovery_point().to_str().ok_or_else(|| {
                MigrationOperationError::new("legacy trust backup path is not valid UTF-8")
            })?;
            MigrationBackup::new(
                reference,
                verified.artifact_sha256(),
                verified.artifact_size_bytes(),
            )
        })
    }

    fn prepare_target<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        let reference = format!("stackctl-ca:{}", self.target_identity.sha256_hex());
        Box::pin(async move {
            V7MigrationAdapterTarget::resource(reference).map_err(MigrationOperationError::new)
        })
    }

    fn cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            let expected = format!("stackctl-ca:{}", self.target_identity.sha256_hex());
            if checkpoint.target_reference() != Some(expected.as_str()) {
                return Err(MigrationOperationError::new(
                    "Stackctl CA target differs from the prepared trust identity",
                ));
            }
            ensure_ca_trusted(
                self.trust_store,
                &self.target_identity,
                self.target_certificate_path,
            )
            .map(|_| ())
            .map_err(|error| operation_error("install Stackctl CA trust", error))
        })
    }

    fn rollback<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            let (recovery_point, bundle) = self.verified_bundle(checkpoint)?;
            for (index, encoded) in bundle.certificates_base64.iter().enumerate() {
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|error| operation_error("decode legacy CA", error))?;
                let pem = std::str::from_utf8(&bytes)
                    .map_err(|_| MigrationOperationError::new("legacy CA PEM is invalid UTF-8"))?;
                let identity = LocalCaIdentity::from_pem(pem)
                    .map_err(|error| operation_error("parse legacy CA", error))?;
                let digest = hex::encode(Sha256::digest(&bytes));
                let path = recovery_point.join(format!(".restore-ca-{index}-{digest}.pem"));
                write_private_certificate(&path, &bytes)?;
                let result = ensure_ca_trusted(self.trust_store, &identity, &path)
                    .map_err(|error| operation_error("restore legacy CA trust", error));
                let cleanup = std::fs::remove_file(&path)
                    .map_err(|error| operation_error("remove temporary legacy CA", error));
                result?;
                cleanup?;
            }

            Ok(())
        })
    }

    fn confirm<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        // Legacy CA removal is installation-scoped and waits for all projects.
        Box::pin(async { Ok(()) })
    }
}

fn validate_accepted_artifacts(
    options: &V7InstallationTrustMigrationAdapterOptions<'_>,
) -> Result<(), String> {
    let inventory = serde_json::from_str::<serde_json::Value>(options.accepted.inventory_json())
        .map_err(|error| format!("accepted v7 trust evidence is invalid: {error}"))?;
    let accepted = inventory
        .pointer("/host_artifacts/caddy_ca_certificates")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "accepted v7 inventory has no Caddy CA evidence".to_owned())?;
    let mut expected = options
        .legacy_ca_artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.path().to_string_lossy().into_owned(),
                artifact.revision().to_owned(),
                artifact.size_bytes(),
            )
        })
        .collect::<Vec<_>>();
    let mut actual = accepted
        .iter()
        .map(|artifact| {
            Ok((
                artifact
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "accepted Caddy CA path is invalid".to_owned())?
                    .to_owned(),
                artifact
                    .get("revision")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "accepted Caddy CA revision is invalid".to_owned())?
                    .to_owned(),
                artifact
                    .get("size_bytes")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| "accepted Caddy CA size is invalid".to_owned())?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    expected.sort();
    actual.sort();
    if expected != actual {
        return Err("legacy CA artifacts do not match accepted v7 evidence".to_owned());
    }

    Ok(())
}

fn read_certificate(path: &Path) -> Result<String, String> {
    let artifact = read_regular(path, 1024 * 1024, true)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "required CA certificate disappeared".to_owned())?;
    String::from_utf8(artifact.bytes)
        .map_err(|_| "CA certificate is not valid UTF-8 PEM".to_owned())
}

#[cfg(unix)]
fn write_private_certificate(path: &Path, bytes: &[u8]) -> Result<(), MigrationOperationError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(MigrationOperationError::new(
                    "temporary legacy CA path is not a regular non-symlink file",
                ));
            }
            let existing = std::fs::read(path)
                .map_err(|error| operation_error("read temporary legacy CA", error))?;
            if existing != bytes {
                return Err(MigrationOperationError::new(
                    "temporary legacy CA path contains unexpected data",
                ));
            }
            std::fs::remove_file(path)
                .map_err(|error| operation_error("remove stale temporary legacy CA", error))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(operation_error("inspect temporary legacy CA path", error));
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| operation_error("create temporary legacy CA", error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| operation_error("write temporary legacy CA", error))
}

#[cfg(not(unix))]
fn write_private_certificate(_path: &Path, _bytes: &[u8]) -> Result<(), MigrationOperationError> {
    Err(MigrationOperationError::new(
        "secure legacy CA restore is unavailable on this platform",
    ))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyTrustBundle {
    schema_version: u32,
    certificates_base64: Vec<String>,
}
