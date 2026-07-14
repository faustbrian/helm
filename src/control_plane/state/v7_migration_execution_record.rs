use super::{
    V7MigrationAdapterCheckpoint, V7MigrationAdapterCheckpointPhase, V7MigrationExecutionPhase,
    V7MigrationExecutionRecordOptions,
};
use std::path::Path;

/// Atomic project-wide barrier preventing partial v7 cutover.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7MigrationExecutionRecord {
    options: V7MigrationExecutionRecordOptions,
}

impl V7MigrationExecutionRecord {
    pub(crate) fn new(mut options: V7MigrationExecutionRecordOptions) -> Result<Self, String> {
        options
            .checkpoints
            .sort_by(|left, right| left.adapter_id().cmp(right.adapter_id()));
        let valid_identity = !options.project_id.is_empty()
            && options.canonical_project_path.is_absolute()
            && valid_sha256(&options.evidence_revision)
            && valid_sha256(&options.adapter_plan_revision)
            && options.updated_at_unix_seconds >= 0
            && !options.checkpoints.is_empty()
            && options
                .checkpoints
                .iter()
                .all(|checkpoint| checkpoint.validate().is_ok())
            && !options
                .checkpoints
                .windows(2)
                .any(|pair| pair[0].adapter_id() == pair[1].adapter_id());
        if !valid_identity {
            return Err(
                "v7 migration execution requires absolute accepted identity, unique adapters, SHA-256 revisions, and non-negative time"
                    .to_owned(),
            );
        }
        validate_phase(options.phase, &options.checkpoints)?;

        Ok(Self { options })
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.options.project_id
    }

    pub(crate) fn canonical_project_path(&self) -> &Path {
        &self.options.canonical_project_path
    }

    pub(crate) fn evidence_revision(&self) -> &str {
        &self.options.evidence_revision
    }

    pub(crate) fn adapter_plan_revision(&self) -> &str {
        &self.options.adapter_plan_revision
    }

    pub(crate) const fn phase(&self) -> V7MigrationExecutionPhase {
        self.options.phase
    }

    pub(crate) fn checkpoints(&self) -> &[V7MigrationAdapterCheckpoint] {
        &self.options.checkpoints
    }

    pub(crate) const fn updated_at_unix_seconds(&self) -> i64 {
        self.options.updated_at_unix_seconds
    }

    pub(super) fn has_same_identity(&self, other: &Self) -> bool {
        self.project_id() == other.project_id()
            && self.canonical_project_path() == other.canonical_project_path()
            && self.evidence_revision() == other.evidence_revision()
            && self.adapter_plan_revision() == other.adapter_plan_revision()
    }

    pub(super) fn checkpoints_json(&self) -> Result<String, String> {
        serde_json::to_string(self.checkpoints())
            .map_err(|error| format!("serialize v7 migration checkpoints: {error}"))
    }
}

fn validate_phase(
    phase: V7MigrationExecutionPhase,
    checkpoints: &[V7MigrationAdapterCheckpoint],
) -> Result<(), String> {
    let matches = match phase {
        V7MigrationExecutionPhase::Planned => checkpoints
            .iter()
            .all(|checkpoint| checkpoint.phase() == V7MigrationAdapterCheckpointPhase::Pending),
        V7MigrationExecutionPhase::Preparing => {
            checkpoints
                .iter()
                .any(|checkpoint| checkpoint.phase() != V7MigrationAdapterCheckpointPhase::Pending)
                && checkpoints.iter().all(|checkpoint| {
                    checkpoint.phase() <= V7MigrationAdapterCheckpointPhase::TargetVerified
                })
        }
        V7MigrationExecutionPhase::Prepared => checkpoints.iter().all(|checkpoint| {
            checkpoint.phase() == V7MigrationAdapterCheckpointPhase::TargetVerified
        }),
        V7MigrationExecutionPhase::Cutover => checkpoints
            .iter()
            .all(|checkpoint| checkpoint.phase() == V7MigrationAdapterCheckpointPhase::Cutover),
        V7MigrationExecutionPhase::Confirmed => checkpoints
            .iter()
            .all(|checkpoint| checkpoint.phase() == V7MigrationAdapterCheckpointPhase::Confirmed),
        V7MigrationExecutionPhase::RolledBack => checkpoints
            .iter()
            .all(|checkpoint| checkpoint.phase() == V7MigrationAdapterCheckpointPhase::RolledBack),
    };
    if matches {
        Ok(())
    } else {
        let requirement = match phase {
            V7MigrationExecutionPhase::Prepared => "every adapter target to be verified",
            V7MigrationExecutionPhase::Planned => "every adapter to be pending",
            V7MigrationExecutionPhase::Preparing => "bounded adapter preparation progress",
            V7MigrationExecutionPhase::Cutover => "every adapter to complete cutover",
            V7MigrationExecutionPhase::Confirmed => "every adapter to be confirmed",
            V7MigrationExecutionPhase::RolledBack => "every adapter to be rolled back",
        };
        Err(format!(
            "v7 migration execution phase '{}' requires {requirement}",
            phase.label()
        ))
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
