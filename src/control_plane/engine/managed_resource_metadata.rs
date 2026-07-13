use super::{EngineError, ManagedResourceMetadataOptions, ResourceKind, RetentionClass};
use std::collections::BTreeMap;

pub(super) const MANAGED_LABEL: &str = "dev.stackctl.managed";
pub(super) const INSTALLATION_LABEL: &str = "dev.stackctl.installation";
pub(super) const KIND_LABEL: &str = "dev.stackctl.kind";
pub(super) const PROJECT_LABEL: &str = "dev.stackctl.project";
pub(super) const RESOURCE_LABEL: &str = "dev.stackctl.resource";
pub(super) const FINGERPRINT_LABEL: &str = "dev.stackctl.fingerprint";
pub(super) const SCHEMA_LABEL: &str = "dev.stackctl.schema";
pub(super) const DESIRED_LABEL: &str = "dev.stackctl.desired";
pub(super) const RETENTION_LABEL: &str = "dev.stackctl.retention";

/// Mandatory ownership metadata attached to every v8 Engine resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManagedResourceMetadata {
    installation_id: String,
    kind: ResourceKind,
    project_id: Option<String>,
    resource_id: Option<String>,
    compatibility_fingerprint: String,
    schema_version: u32,
    desired_revision: String,
    retention: RetentionClass,
}

impl ManagedResourceMetadata {
    /// Validates complete ownership metadata before any Engine request.
    pub(crate) fn new(options: ManagedResourceMetadataOptions) -> Result<Self, EngineError> {
        if options.installation_id.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: "managed resource installation ID must not be empty".to_owned(),
            });
        }

        if options.project_id.as_ref().is_some_and(String::is_empty) {
            return Err(EngineError::InvalidRequest {
                detail: "managed resource project ID must not be empty".to_owned(),
            });
        }

        if options.compatibility_fingerprint.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: "managed resource compatibility fingerprint must not be empty".to_owned(),
            });
        }

        if options.schema_version == 0 {
            return Err(EngineError::InvalidRequest {
                detail: "managed resource schema version must be greater than zero".to_owned(),
            });
        }

        if options.desired_revision.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: "managed resource desired revision must not be empty".to_owned(),
            });
        }

        Ok(Self {
            installation_id: options.installation_id,
            kind: options.kind,
            project_id: options.project_id,
            resource_id: None,
            compatibility_fingerprint: options.compatibility_fingerprint,
            schema_version: options.schema_version,
            desired_revision: options.desired_revision,
            retention: options.retention,
        })
    }

    /// Adds a stable identity for repeated resources of the same kind.
    pub(crate) fn with_resource_id(
        mut self,
        resource_id: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let resource_id = resource_id.into();
        if !is_valid_resource_id(&resource_id) {
            return Err(EngineError::InvalidRequest {
                detail: format!(
                    "managed resource identity '{resource_id}' must be a lowercase DNS label"
                ),
            });
        }

        self.resource_id = Some(resource_id);

        Ok(self)
    }

    /// Produces the complete reserved label set consumed by adapters.
    pub(crate) fn labels(&self) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::from([
            (MANAGED_LABEL.to_owned(), "true".to_owned()),
            (INSTALLATION_LABEL.to_owned(), self.installation_id.clone()),
            (KIND_LABEL.to_owned(), self.kind.label().to_owned()),
            (
                FINGERPRINT_LABEL.to_owned(),
                self.compatibility_fingerprint.clone(),
            ),
            (SCHEMA_LABEL.to_owned(), self.schema_version.to_string()),
            (DESIRED_LABEL.to_owned(), self.desired_revision.clone()),
            (
                RETENTION_LABEL.to_owned(),
                self.retention.label().to_owned(),
            ),
        ]);

        if let Some(project_id) = &self.project_id {
            labels.insert(PROJECT_LABEL.to_owned(), project_id.clone());
        }
        if let Some(resource_id) = &self.resource_id {
            labels.insert(RESOURCE_LABEL.to_owned(), resource_id.clone());
        }

        labels
    }

    pub(crate) fn installation_id(&self) -> &str {
        &self.installation_id
    }

    pub(crate) const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub(crate) const fn kind(&self) -> ResourceKind {
        self.kind
    }

    pub(crate) fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub(crate) fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }

    pub(crate) const fn retention(&self) -> RetentionClass {
        self.retention
    }
}

fn is_valid_resource_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}
