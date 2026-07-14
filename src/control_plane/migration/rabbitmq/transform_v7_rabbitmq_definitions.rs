use super::V7RabbitMqCredential;
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::RabbitMqProjectDefinition;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

const HASHING_ALGORITHM: &str = "rabbit_password_hashing_sha256";
const METADATA_FIELDS: &[&str] = &[
    "rabbit_version",
    "rabbitmq_version",
    "product_name",
    "product_version",
];
const SCOPED_COLLECTIONS: &[&str] = &[
    "parameters",
    "policies",
    "operator_policies",
    "queues",
    "exchanges",
    "bindings",
];

/// Maps one accepted empty-message vhost topology to its deterministic v8 identity.
pub(super) fn transform_v7_rabbitmq_definitions(
    source: &[u8],
    source_vhost: &str,
    source_credential: &V7RabbitMqCredential,
    target: &RabbitMqProjectDefinition,
) -> Result<Vec<u8>, MigrationOperationError> {
    let document = serde_json::from_slice::<Value>(source)
        .map_err(|error| operation_error("v7 RabbitMQ definitions are malformed", error))?;
    let document = document.as_object().ok_or_else(|| {
        MigrationOperationError::new("v7 RabbitMQ definitions must be one JSON object")
    })?;
    validate_top_level(document)?;
    validate_source_user(document, source_credential)?;
    validate_source_vhost(document, source_vhost)?;
    validate_permissions(document, source_vhost, source_credential.username())?;
    let topic_permissions = transform_topic_permissions(
        array(document, "topic_permissions")?,
        source_vhost,
        source_credential.username(),
        target,
    )?;
    if !array(document, "global_parameters")?.is_empty() {
        return Err(MigrationOperationError::new(
            "v7 RabbitMQ definitions contain global parameters that cannot be project-scoped",
        ));
    }

    let mut output = Map::new();
    output.insert(
        "users".to_owned(),
        json!([{
            "name": target.username(),
            "password_hash": target.password_hash().encoded(),
            "hashing_algorithm": HASHING_ALGORITHM,
            "tags": [],
        }]),
    );
    output.insert("vhosts".to_owned(), json!([{"name": target.vhost()}]));
    output.insert(
        "permissions".to_owned(),
        json!([{
            "user": target.username(),
            "vhost": target.vhost(),
            "configure": ".*",
            "write": ".*",
            "read": ".*",
        }]),
    );
    output.insert(
        "topic_permissions".to_owned(),
        Value::Array(topic_permissions),
    );
    output.insert("global_parameters".to_owned(), Value::Array(Vec::new()));
    for field in SCOPED_COLLECTIONS {
        output.insert(
            (*field).to_owned(),
            Value::Array(transform_scoped(
                array(document, field)?,
                field,
                source_vhost,
                target.vhost(),
            )?),
        );
    }

    serde_json::to_vec(&Value::Object(output))
        .map_err(|error| operation_error("v8 RabbitMQ definitions are invalid", error))
}

fn validate_top_level(document: &Map<String, Value>) -> Result<(), MigrationOperationError> {
    let mut known = METADATA_FIELDS.iter().copied().collect::<BTreeSet<_>>();
    known.extend([
        "users",
        "vhosts",
        "permissions",
        "topic_permissions",
        "global_parameters",
    ]);
    known.extend(SCOPED_COLLECTIONS.iter().copied());
    if let Some(field) = document
        .keys()
        .find(|field| !known.contains(field.as_str()))
    {
        return Err(MigrationOperationError::new(format!(
            "v7 RabbitMQ definitions contain unsupported top-level field '{field}'"
        )));
    }
    Ok(())
}

fn validate_source_user(
    document: &Map<String, Value>,
    credential: &V7RabbitMqCredential,
) -> Result<(), MigrationOperationError> {
    let users = array(document, "users")?;
    let matching = users
        .iter()
        .filter(|user| field(user, "name") == Some(credential.username()))
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(MigrationOperationError::new(
            "v7 RabbitMQ definitions do not contain exactly one accepted user",
        ));
    }
    let user = matching[0];
    let hash = field(user, "password_hash").unwrap_or_default();
    if field(user, "hashing_algorithm") != Some(HASHING_ALGORITHM)
        || !credential.matches_password_hash(hash)
    {
        return Err(MigrationOperationError::new(
            "v7 RabbitMQ user credential does not match accepted migration input",
        ));
    }
    Ok(())
}

fn validate_source_vhost(
    document: &Map<String, Value>,
    source_vhost: &str,
) -> Result<(), MigrationOperationError> {
    let vhosts = array(document, "vhosts")?;
    if vhosts.len() != 1 || field(&vhosts[0], "name") != Some(source_vhost) {
        return Err(MigrationOperationError::new(
            "v7 RabbitMQ definitions are not scoped to the exact accepted vhost",
        ));
    }
    Ok(())
}

fn validate_permissions(
    document: &Map<String, Value>,
    source_vhost: &str,
    source_user: &str,
) -> Result<(), MigrationOperationError> {
    let permissions = array(document, "permissions")?;
    if permissions.len() != 1
        || field(&permissions[0], "vhost") != Some(source_vhost)
        || field(&permissions[0], "user") != Some(source_user)
    {
        return Err(MigrationOperationError::new(
            "v7 RabbitMQ permissions are not limited to the accepted user and vhost",
        ));
    }
    Ok(())
}

fn transform_topic_permissions(
    permissions: &[Value],
    source_vhost: &str,
    source_user: &str,
    target: &RabbitMqProjectDefinition,
) -> Result<Vec<Value>, MigrationOperationError> {
    permissions
        .iter()
        .map(|permission| {
            if field(permission, "vhost") != Some(source_vhost)
                || field(permission, "user") != Some(source_user)
            {
                return Err(MigrationOperationError::new(
                    "v7 RabbitMQ topic permission escapes the accepted user or vhost",
                ));
            }
            let mut permission = object(permission, "topic permission")?.clone();
            permission.insert("vhost".to_owned(), Value::String(target.vhost().to_owned()));
            permission.insert(
                "user".to_owned(),
                Value::String(target.username().to_owned()),
            );
            Ok(Value::Object(permission))
        })
        .collect()
}

fn transform_scoped(
    values: &[Value],
    collection: &str,
    source_vhost: &str,
    target_vhost: &str,
) -> Result<Vec<Value>, MigrationOperationError> {
    values
        .iter()
        .map(|value| {
            if field(value, "vhost") != Some(source_vhost) {
                return Err(MigrationOperationError::new(format!(
                    "v7 RabbitMQ {collection} entry escapes the accepted vhost"
                )));
            }
            let mut value = object(value, collection)?.clone();
            value.insert("vhost".to_owned(), Value::String(target_vhost.to_owned()));
            Ok(Value::Object(value))
        })
        .collect()
}

fn array<'document>(
    document: &'document Map<String, Value>,
    field: &str,
) -> Result<&'document [Value], MigrationOperationError> {
    document
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            MigrationOperationError::new(format!(
                "v7 RabbitMQ definitions field '{field}' is missing or malformed"
            ))
        })
}

fn object<'value>(
    value: &'value Value,
    kind: &str,
) -> Result<&'value Map<String, Value>, MigrationOperationError> {
    value.as_object().ok_or_else(|| {
        MigrationOperationError::new(format!("v7 RabbitMQ {kind} entry is malformed"))
    })
}

fn field<'value>(value: &'value Value, field: &str) -> Option<&'value str> {
    value.get(field).and_then(Value::as_str)
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
