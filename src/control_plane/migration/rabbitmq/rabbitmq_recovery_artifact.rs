use crate::control_plane::migration::MigrationOperationError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Component, Path};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const FORMAT_VERSION: u32 = 1;
const HEADER_MAGIC: &[u8; 8] = b"STRMQ001";
const MAX_HEADER_BYTES: usize = 4 * 1_024 * 1_024;
const SCOPED_DEFINITION_SECTIONS: [&str; 7] = [
    "vhosts",
    "parameters",
    "policies",
    "operator_policies",
    "queues",
    "exchanges",
    "bindings",
];
const EXCLUDED_CREDENTIAL_SECTIONS: [&str; 4] = [
    "users",
    "permissions",
    "topic_permissions",
    "global_parameters",
];
const IGNORED_METADATA_FIELDS: [&str; 4] = [
    "rabbit_version",
    "rabbitmq_version",
    "product_name",
    "product_version",
];

/// Versioned RabbitMQ topology header followed by one raw vhost-store tar.
#[derive(Deserialize, Serialize)]
pub(crate) struct RabbitMqRecoveryArtifact {
    format_version: u32,
    vhost: String,
    relative_message_store_path: String,
    definitions: Value,
    queue_messages: BTreeMap<String, u64>,
}

impl RabbitMqRecoveryArtifact {
    pub(crate) fn new(
        vhost: String,
        relative_message_store_path: String,
        definitions: Value,
        queue_messages: BTreeMap<String, u64>,
    ) -> Result<Self, MigrationOperationError> {
        let definitions = sanitize_definitions(definitions, &vhost)?;
        let artifact = Self {
            format_version: FORMAT_VERSION,
            vhost,
            relative_message_store_path,
            definitions,
            queue_messages,
        };
        artifact.validate()?;

        Ok(artifact)
    }

    pub(crate) fn relative_message_store_path(&self) -> &Path {
        Path::new(&self.relative_message_store_path)
    }

    pub(crate) fn vhost(&self) -> &str {
        &self.vhost
    }

    pub(crate) const fn definitions(&self) -> &Value {
        &self.definitions
    }

    pub(crate) const fn queue_messages(&self) -> &BTreeMap<String, u64> {
        &self.queue_messages
    }

    pub(crate) async fn write_to(
        &self,
        output: &mut (impl AsyncWrite + Unpin),
    ) -> Result<(), MigrationOperationError> {
        let header = serde_json::to_vec(self).map_err(|error| {
            MigrationOperationError::new(format!(
                "RabbitMQ recovery header encoding failed: {error}"
            ))
        })?;
        if header.len() > MAX_HEADER_BYTES {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ recovery header exceeds {MAX_HEADER_BYTES} bytes"
            )));
        }
        let header_length = u64::try_from(header.len()).map_err(|error| {
            MigrationOperationError::new(format!(
                "RabbitMQ recovery header length is invalid: {error}"
            ))
        })?;
        output.write_all(HEADER_MAGIC).await.map_err(io_error)?;
        output
            .write_all(&header_length.to_be_bytes())
            .await
            .map_err(io_error)?;
        output.write_all(&header).await.map_err(io_error)
    }

    pub(crate) async fn read_from(
        input: &mut (impl AsyncRead + Unpin),
    ) -> Result<Self, MigrationOperationError> {
        let mut magic = [0_u8; HEADER_MAGIC.len()];
        input.read_exact(&mut magic).await.map_err(io_error)?;
        if &magic != HEADER_MAGIC {
            return Err(MigrationOperationError::new(
                "RabbitMQ recovery artifact has an unsupported header",
            ));
        }
        let mut encoded_length = [0_u8; 8];
        input
            .read_exact(&mut encoded_length)
            .await
            .map_err(io_error)?;
        let header_length =
            usize::try_from(u64::from_be_bytes(encoded_length)).map_err(|error| {
                MigrationOperationError::new(format!(
                    "RabbitMQ recovery header length is invalid: {error}"
                ))
            })?;
        if header_length == 0 || header_length > MAX_HEADER_BYTES {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ recovery header length must be between 1 and {MAX_HEADER_BYTES} bytes"
            )));
        }
        let mut header = vec![0_u8; header_length];
        input.read_exact(&mut header).await.map_err(io_error)?;
        let artifact: Self = serde_json::from_slice(&header).map_err(|error| {
            MigrationOperationError::new(format!(
                "RabbitMQ recovery header is invalid JSON: {error}"
            ))
        })?;
        artifact.validate()?;

        Ok(artifact)
    }

    fn validate(&self) -> Result<(), MigrationOperationError> {
        let path = Path::new(&self.relative_message_store_path);
        let expected_prefix = Path::new("mnesia/rabbit@localhost/msg_stores/vhosts");
        let valid_path = path.starts_with(expected_prefix)
            && path.components().count() == expected_prefix.components().count() + 1
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if self.format_version != FORMAT_VERSION
            || self.vhost.is_empty()
            || !valid_path
            || !self.definitions.is_object()
            || self.queue_messages.keys().any(String::is_empty)
            || !definitions_are_scoped_to_vhost(&self.definitions, &self.vhost)
        {
            return Err(MigrationOperationError::new(
                "RabbitMQ recovery artifact header is incomplete or unsafe",
            ));
        }

        Ok(())
    }
}

fn definitions_are_scoped_to_vhost(definitions: &Value, vhost: &str) -> bool {
    let Some(definitions) = definitions.as_object() else {
        return false;
    };
    let Some([declared_vhost]) = definitions
        .get("vhosts")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    else {
        return false;
    };
    if declared_vhost.get("name").and_then(Value::as_str) != Some(vhost) {
        return false;
    }

    definitions.keys().all(|key| {
        SCOPED_DEFINITION_SECTIONS.contains(&key.as_str())
            && definitions.get(key).is_some_and(Value::is_array)
    }) && definitions
        .iter()
        .filter(|(section, _)| section.as_str() != "vhosts")
        .all(|(_, section)| {
            section.as_array().is_some_and(|entries| {
                entries
                    .iter()
                    .all(|entry| entry.get("vhost").and_then(Value::as_str) == Some(vhost))
            })
        })
}

fn sanitize_definitions(definitions: Value, vhost: &str) -> Result<Value, MigrationOperationError> {
    let definitions = definitions.as_object().ok_or_else(|| {
        MigrationOperationError::new("RabbitMQ definitions export is not an object")
    })?;
    let mut sanitized = serde_json::Map::new();
    for (section, value) in definitions {
        if SCOPED_DEFINITION_SECTIONS.contains(&section.as_str()) {
            sanitized.insert(section.clone(), value.clone());
        } else if !EXCLUDED_CREDENTIAL_SECTIONS.contains(&section.as_str())
            && !IGNORED_METADATA_FIELDS.contains(&section.as_str())
        {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ definitions export contains unsupported section '{section}'"
            )));
        }
    }
    let sanitized = Value::Object(sanitized);
    if !definitions_are_scoped_to_vhost(&sanitized, vhost) {
        return Err(MigrationOperationError::new(
            "RabbitMQ definitions export contains unscoped or incomplete topology",
        ));
    }

    Ok(sanitized)
}

fn io_error(error: std::io::Error) -> MigrationOperationError {
    MigrationOperationError::new(format!("RabbitMQ recovery artifact I/O failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{RabbitMqRecoveryArtifact, definitions_are_scoped_to_vhost};
    use std::collections::BTreeMap;

    #[test]
    fn recovery_artifact_excludes_users_and_permissions_from_topology() {
        let artifact = RabbitMqRecoveryArtifact::new(
            "stackctl_bill_database".to_owned(),
            "mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P".to_owned(),
            serde_json::json!({
                "rabbitmq_version": "4.3.0",
                "users": [{"name": "st_bill_database", "password_hash": "stale"}],
                "permissions": [{
                    "user": "st_bill_database",
                    "vhost": "stackctl_bill_database",
                    "configure": ".*",
                    "write": ".*",
                    "read": ".*"
                }],
                "vhosts": [{"name": "stackctl_bill_database"}],
                "queues": [{"name": "jobs", "vhost": "stackctl_bill_database"}]
            }),
            BTreeMap::from([("jobs".to_owned(), 2)]),
        )
        .expect("sanitized recovery artifact");

        assert!(artifact.definitions().get("users").is_none());
        assert!(artifact.definitions().get("permissions").is_none());
        assert!(artifact.definitions().get("queues").is_some());
    }

    #[test]
    fn recovery_artifact_rejects_unknown_unscoped_definition_sections() {
        let result = RabbitMqRecoveryArtifact::new(
            "stackctl_bill_database".to_owned(),
            "mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P".to_owned(),
            serde_json::json!({
                "vhosts": [{"name": "stackctl_bill_database"}],
                "foreign_objects": [{"name": "unsafe"}]
            }),
            BTreeMap::new(),
        );
        let error = result
            .err()
            .expect("unknown unscoped definitions must fail closed");

        assert!(error.to_string().contains("unsupported section"));
    }

    #[test]
    fn stored_definition_header_rejects_credential_sections() {
        let definitions = serde_json::json!({
            "vhosts": [{"name": "stackctl_bill_database"}],
            "users": [{"name": "st_bill_database", "password_hash": "stale"}]
        });

        assert!(!definitions_are_scoped_to_vhost(
            &definitions,
            "stackctl_bill_database"
        ));
    }
}
