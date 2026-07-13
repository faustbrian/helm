use super::{ArtifactLock, ArtifactLockError};
use serde::Deserialize;
use serde_yaml_ng::{Deserializer, Value};
use std::path::Path;

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Parses exactly one restricted YAML document into the v8 artifact-lock model.
pub(crate) fn parse_artifact_lock(
    source: &str,
    lock_path: &Path,
) -> Result<ArtifactLock, ArtifactLockError> {
    let mut documents = Deserializer::from_str(source);
    let first_document = documents.next().ok_or_else(|| {
        ArtifactLockError::new(
            lock_path.to_path_buf(),
            "expected exactly one YAML document",
        )
    })?;
    let value = Value::deserialize(first_document).map_err(|error| {
        ArtifactLockError::new(lock_path.to_path_buf(), normalize_yaml_error(&error))
    })?;

    if documents.next().is_some() {
        return Err(ArtifactLockError::new(
            lock_path.to_path_buf(),
            "expected exactly one YAML document",
        ));
    }

    reject_yaml_tags(&value, lock_path)?;

    let lock = serde_yaml_ng::from_value::<ArtifactLock>(value).map_err(|error| {
        ArtifactLockError::new(lock_path.to_path_buf(), normalize_yaml_error(&error))
    })?;

    if lock.schema_version() != SUPPORTED_SCHEMA_VERSION {
        return Err(ArtifactLockError::new(
            lock_path.to_path_buf(),
            format!(
                "schema_version {} is unsupported; expected {SUPPORTED_SCHEMA_VERSION}",
                lock.schema_version()
            ),
        ));
    }

    Ok(lock)
}

fn reject_yaml_tags(value: &Value, lock_path: &Path) -> Result<(), ArtifactLockError> {
    match value {
        Value::Sequence(values) => {
            for value in values {
                reject_yaml_tags(value, lock_path)?;
            }
        }
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                reject_yaml_tags(key, lock_path)?;
                reject_yaml_tags(value, lock_path)?;
            }
        }
        Value::Tagged(_) => {
            return Err(ArtifactLockError::new(
                lock_path.to_path_buf(),
                "YAML tags are not supported",
            ));
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }

    Ok(())
}

fn normalize_yaml_error(error: &serde_yaml_ng::Error) -> String {
    let detail = error.to_string();
    let duplicate_prefix = "duplicate entry with key \"";

    if let Some(field) = detail
        .strip_prefix(duplicate_prefix)
        .and_then(|remainder| remainder.strip_suffix('"'))
    {
        return format!("duplicate field `{field}`");
    }

    detail
}
