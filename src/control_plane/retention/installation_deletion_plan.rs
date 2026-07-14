use super::{
    InstallationDeletionPlanOptions, InstallationVolumeDeletion, LogicalPrunePlan,
    LogicalPrunePlanOptions,
};
use crate::control_plane::state::{
    LogicalResourceRecord, RecoveryPointRecord, ResourceRecord, ResourceRetention,
};
use sha2::{Digest, Sha256};

/// Deterministic, secret-free proof that every retained tenant is recoverable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstallationDeletionPlan {
    logical_prunes: Vec<LogicalPrunePlan>,
    volume_deletions: Vec<InstallationVolumeDeletion>,
    confirmation_token: String,
}

impl InstallationDeletionPlan {
    pub(crate) fn new(options: InstallationDeletionPlanOptions<'_>) -> Result<Self, String> {
        if options.installation_id.is_empty() {
            return Err("installation deletion identity must not be empty".to_owned());
        }
        let mut logical_resources = options.logical_resources.iter().collect::<Vec<_>>();
        logical_resources.sort_by_key(|logical| {
            (
                logical.project_id(),
                logical.service_id(),
                logical.logical_resource_id(),
            )
        });
        let logical_prunes = logical_resources
            .into_iter()
            .map(|logical| build_logical_prune(&options, logical))
            .collect::<Result<Vec<_>, _>>()?;
        let mut volumes = options
            .resources
            .iter()
            .filter(|resource| {
                resource.kind() == "volume"
                    && resource.retention() == ResourceRetention::Persistent
                    && resource.project_id().is_some()
            })
            .collect::<Vec<_>>();
        volumes.sort_by_key(|resource| resource.resource_id());
        let volume_deletions = volumes
            .into_iter()
            .map(|resource| build_volume_deletion(&options, resource))
            .collect::<Result<Vec<_>, _>>()?;
        let confirmation_token =
            confirmation_token(options.installation_id, &logical_prunes, &volume_deletions);

        Ok(Self {
            logical_prunes,
            volume_deletions,
            confirmation_token,
        })
    }

    pub(crate) fn logical_prunes(&self) -> &[LogicalPrunePlan] {
        &self.logical_prunes
    }

    pub(crate) fn confirmation_token(&self) -> &str {
        &self.confirmation_token
    }

    pub(crate) fn volume_deletions(&self) -> &[InstallationVolumeDeletion] {
        &self.volume_deletions
    }
}

fn confirmation_token(
    installation_id: &str,
    logical_prunes: &[LogicalPrunePlan],
    volume_deletions: &[InstallationVolumeDeletion],
) -> String {
    let mut hasher = Sha256::new();
    for field in std::iter::once("stackctl-installation-delete-v1")
        .chain(std::iter::once(installation_id))
        .chain(
            logical_prunes
                .iter()
                .map(LogicalPrunePlan::confirmation_token),
        )
        .chain(
            volume_deletions
                .iter()
                .map(InstallationVolumeDeletion::confirmation_token),
        )
    {
        hasher.update(field.len().to_be_bytes());
        hasher.update(field.as_bytes());
    }

    hex::encode(hasher.finalize())
}

fn build_volume_deletion(
    options: &InstallationDeletionPlanOptions<'_>,
    resource: &ResourceRecord,
) -> Result<InstallationVolumeDeletion, String> {
    let recovery =
        latest_exact_volume_recovery(options.recovery_points, resource).ok_or_else(|| {
            format!(
                "persistent volume '{}' has no exact verified recovery point",
                resource.resource_id()
            )
        })?;

    InstallationVolumeDeletion::new(resource, recovery)
}

fn latest_exact_volume_recovery<'state>(
    recovery_points: &'state [RecoveryPointRecord],
    resource: &ResourceRecord,
) -> Option<&'state RecoveryPointRecord> {
    recovery_points
        .iter()
        .filter(|recovery| {
            recovery.project_id() == resource.project_id().unwrap_or_default()
                && recovery.service_id() == resource.scope_id().unwrap_or_default()
                && recovery.logical_resource_id() == resource.resource_id()
                && recovery.resource_kind() == resource.kind()
                && recovery.compatibility_fingerprint() == resource.compatibility_fingerprint()
        })
        .max_by_key(|recovery| {
            (
                recovery.created_at_unix_seconds(),
                recovery.verified_at_unix_seconds(),
                recovery.recovery_point_id(),
            )
        })
}

fn build_logical_prune(
    options: &InstallationDeletionPlanOptions<'_>,
    logical: &LogicalResourceRecord,
) -> Result<LogicalPrunePlan, String> {
    let recovery = latest_exact_recovery(options.recovery_points, logical).ok_or_else(|| {
        format!(
            "logical resource '{}' has no exact verified recovery point",
            logical.logical_resource_id()
        )
    })?;
    LogicalPrunePlan::for_installation_deletion(LogicalPrunePlanOptions {
        installation_id: options.installation_id,
        project_id: logical.project_id(),
        service_id: logical.service_id(),
        recovery_point_id: recovery.recovery_point_id(),
        project_registered: false,
        logical_resources: options.logical_resources,
        credentials: options.credentials,
        recovery_points: options.recovery_points,
    })
}

fn latest_exact_recovery<'state>(
    recovery_points: &'state [RecoveryPointRecord],
    logical: &LogicalResourceRecord,
) -> Option<&'state RecoveryPointRecord> {
    recovery_points
        .iter()
        .filter(|recovery| {
            recovery.project_id() == logical.project_id()
                && recovery.service_id() == logical.service_id()
                && recovery.logical_resource_id() == logical.logical_resource_id()
                && recovery.resource_kind() == logical.kind()
                && recovery.compatibility_fingerprint() == logical.compatibility_fingerprint()
        })
        .max_by_key(|recovery| {
            (
                recovery.created_at_unix_seconds(),
                recovery.verified_at_unix_seconds(),
                recovery.recovery_point_id(),
            )
        })
}
