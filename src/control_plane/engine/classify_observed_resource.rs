use super::managed_resource_metadata::{
    DESIRED_LABEL, FINGERPRINT_LABEL, INSTALLATION_LABEL, KIND_LABEL, MANAGED_LABEL, PROJECT_LABEL,
    RETENTION_LABEL, SCHEMA_LABEL,
};
use super::{
    ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedResourceOwnership,
    ResourceKind, RetentionClass,
};
use std::collections::BTreeMap;

/// Reconstructs ownership from labels without adopting ambiguous objects.
pub(crate) fn classify_observed_resource(
    labels: &BTreeMap<String, String>,
    current_installation_id: &str,
    supported_schema: u32,
) -> ObservedResourceOwnership {
    if labels.get(MANAGED_LABEL).map(String::as_str) != Some("true") {
        return ObservedResourceOwnership::Unmanaged;
    }

    let installation_id = match required_label(labels, INSTALLATION_LABEL) {
        Ok(value) => value,
        Err(ownership) => return ownership,
    };

    if installation_id != current_installation_id {
        return ObservedResourceOwnership::ForeignInstallation {
            installation_id: installation_id.to_owned(),
        };
    }

    let schema_version = match parse_schema(labels) {
        Ok(value) => value,
        Err(ownership) => return ownership,
    };

    if schema_version != supported_schema {
        return ObservedResourceOwnership::UnsupportedSchema {
            found: schema_version,
            supported: supported_schema,
        };
    }

    let kind = match parse_kind(labels) {
        Ok(value) => value,
        Err(ownership) => return ownership,
    };
    let compatibility_fingerprint = match required_label(labels, FINGERPRINT_LABEL) {
        Ok(value) => value.to_owned(),
        Err(ownership) => return ownership,
    };
    let desired_revision = match required_label(labels, DESIRED_LABEL) {
        Ok(value) => value.to_owned(),
        Err(ownership) => return ownership,
    };
    let retention = match parse_retention(labels) {
        Ok(value) => value,
        Err(ownership) => return ownership,
    };

    match ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.to_owned(),
        kind,
        project_id: labels.get(PROJECT_LABEL).cloned(),
        compatibility_fingerprint,
        schema_version,
        desired_revision,
        retention,
    }) {
        Ok(metadata) => ObservedResourceOwnership::Owned(metadata),
        Err(error) => ObservedResourceOwnership::Malformed {
            detail: error.to_string(),
        },
    }
}

fn required_label<'labels>(
    labels: &'labels BTreeMap<String, String>,
    name: &str,
) -> Result<&'labels str, ObservedResourceOwnership> {
    labels
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ObservedResourceOwnership::Malformed {
            detail: format!("managed resource label '{name}' is missing"),
        })
}

fn parse_schema(labels: &BTreeMap<String, String>) -> Result<u32, ObservedResourceOwnership> {
    let value = required_label(labels, SCHEMA_LABEL)?;

    value
        .parse::<u32>()
        .map_err(|_| ObservedResourceOwnership::Malformed {
            detail: format!("managed resource label '{SCHEMA_LABEL}' is not a valid version"),
        })
}

fn parse_kind(
    labels: &BTreeMap<String, String>,
) -> Result<ResourceKind, ObservedResourceOwnership> {
    let value = required_label(labels, KIND_LABEL)?;

    ResourceKind::from_label(value).ok_or_else(|| ObservedResourceOwnership::Malformed {
        detail: format!("managed resource label '{KIND_LABEL}' has unknown value '{value}'"),
    })
}

fn parse_retention(
    labels: &BTreeMap<String, String>,
) -> Result<RetentionClass, ObservedResourceOwnership> {
    let value = required_label(labels, RETENTION_LABEL)?;

    RetentionClass::from_label(value).ok_or_else(|| ObservedResourceOwnership::Malformed {
        detail: format!("managed resource label '{RETENTION_LABEL}' has unknown value '{value}'"),
    })
}
