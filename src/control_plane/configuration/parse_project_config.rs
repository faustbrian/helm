use super::{ConfigParseError, RawProjectConfig};
use serde::Deserialize;
use serde_yaml_ng::{Deserializer, Value};
use std::path::Path;

const SUPPORTED_SCHEMA_VERSION: u32 = 8;

/// Parses exactly one restricted YAML document into the v8 raw model.
pub(crate) fn parse_project_config(
    source: &str,
    config_path: &Path,
) -> Result<RawProjectConfig, ConfigParseError> {
    let mut documents = Deserializer::from_str(source);
    let first_document = documents.next().ok_or_else(|| {
        ConfigParseError::new(
            config_path.to_path_buf(),
            "expected exactly one YAML document",
        )
    })?;
    let value = Value::deserialize(first_document).map_err(|error| {
        ConfigParseError::new(config_path.to_path_buf(), normalize_yaml_error(&error))
    })?;

    if documents.next().is_some() {
        return Err(ConfigParseError::new(
            config_path.to_path_buf(),
            "expected exactly one YAML document",
        ));
    }

    reject_yaml_tags(&value, config_path)?;
    validate_service_version_types(&value, config_path)?;

    let config = serde_yaml_ng::from_value::<RawProjectConfig>(value).map_err(|error| {
        ConfigParseError::new(config_path.to_path_buf(), normalize_yaml_error(&error))
    })?;

    if config.schema_version() != SUPPORTED_SCHEMA_VERSION {
        return Err(ConfigParseError::new(
            config_path.to_path_buf(),
            format!(
                "schema_version {} is unsupported; expected {SUPPORTED_SCHEMA_VERSION}",
                config.schema_version()
            ),
        ));
    }

    Ok(config)
}

fn reject_yaml_tags(value: &Value, config_path: &Path) -> Result<(), ConfigParseError> {
    match value {
        Value::Sequence(values) => {
            for value in values {
                reject_yaml_tags(value, config_path)?;
            }
        }
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                reject_yaml_tags(key, config_path)?;
                reject_yaml_tags(value, config_path)?;
            }
        }
        Value::Tagged(_) => {
            return Err(ConfigParseError::new(
                config_path.to_path_buf(),
                "YAML tags are not supported",
            ));
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }

    Ok(())
}

fn validate_service_version_types(
    value: &Value,
    config_path: &Path,
) -> Result<(), ConfigParseError> {
    let Some(services) = mapping_value(value, "services").and_then(Value::as_mapping) else {
        return Ok(());
    };

    for (service_name, service) in services {
        let Some(service_name) = service_name.as_str() else {
            continue;
        };
        let Some(version) = mapping_value(service, "version") else {
            continue;
        };

        if !matches!(version, Value::String(_)) {
            return Err(ConfigParseError::new(
                config_path.to_path_buf(),
                format!("services.{service_name}.version must be a string"),
            ));
        }
    }

    Ok(())
}

fn mapping_value<'value>(value: &'value Value, key: &str) -> Option<&'value Value> {
    value
        .as_mapping()
        .and_then(|mapping| mapping.get(Value::String(key.to_owned())))
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
