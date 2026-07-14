use super::AcceptedV7InventoryRecordOptions;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Immutable accepted legacy inventory used to gate later migration work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AcceptedV7InventoryRecord {
    options: AcceptedV7InventoryRecordOptions,
    evidence_revision: String,
    requires_generated_environment_rollback: bool,
}

impl AcceptedV7InventoryRecord {
    pub(crate) fn new(options: AcceptedV7InventoryRecordOptions) -> Result<Self, String> {
        let inventory = serde_json::from_str::<serde_json::Value>(&options.inventory_json)
            .map_err(|error| format!("accepted v7 inventory JSON is invalid: {error}"))?;
        let matching_identity = inventory.as_object().is_some_and(|object| {
            object.get("project_id").and_then(|value| value.as_str())
                == Some(options.project_id.as_str())
                && object
                    .get("canonical_project_path")
                    .and_then(|value| value.as_str())
                    == options.canonical_project_path.to_str()
                && object
                    .get("source_revision")
                    .and_then(|value| value.as_str())
                    == Some(options.source_revision.as_str())
                && object
                    .get("blockers")
                    .and_then(|value| value.as_array())
                    .is_some_and(Vec::is_empty)
        });
        let generated_environment_present = inventory
            .pointer("/host_artifacts/generated_environment")
            .is_some_and(|value| !value.is_null());
        let valid = !options.project_id.is_empty()
            && options.canonical_project_path.is_absolute()
            && is_source_revision(&options.source_revision)
            && matching_identity
            && options.accepted_at_unix_seconds >= 0;
        if !valid {
            return Err(
                "accepted v7 inventory requires matching blocker-free object evidence, an absolute path, complete identity, valid source revision, and non-negative timestamp"
                    .to_owned(),
            );
        }
        let evidence_revision = hex::encode(Sha256::digest(options.inventory_json.as_bytes()));

        Ok(Self {
            options,
            evidence_revision,
            requires_generated_environment_rollback: generated_environment_present,
        })
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.options.project_id
    }

    pub(crate) fn canonical_project_path(&self) -> &Path {
        &self.options.canonical_project_path
    }

    pub(crate) fn source_revision(&self) -> &str {
        &self.options.source_revision
    }

    pub(crate) fn evidence_revision(&self) -> &str {
        &self.evidence_revision
    }

    pub(crate) fn inventory_json(&self) -> &str {
        &self.options.inventory_json
    }

    pub(crate) const fn generated_environment_rollback(
        &self,
    ) -> Option<&super::AcceptedV7EnvironmentRollback> {
        self.options.generated_environment_rollback.as_ref()
    }

    pub(crate) const fn requires_generated_environment_rollback(&self) -> bool {
        self.requires_generated_environment_rollback
    }

    pub(crate) const fn accepted_at_unix_seconds(&self) -> i64 {
        self.options.accepted_at_unix_seconds
    }
}

fn is_source_revision(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
