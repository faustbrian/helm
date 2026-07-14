use super::{DataLifecycleStrategy, LogicalPrunePlanOptions, resolve_data_lifecycle_strategy};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, LogicalResourceRecord, RecoveryPointRecord,
    ResourceLifecycle,
};
use sha2::{Digest, Sha256};

/// Secret-free exact intent regenerated before any destructive adapter runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogicalPrunePlan {
    strategy: DataLifecycleStrategy,
    project_id: String,
    service_id: String,
    logical_resource_id: String,
    shared_resource_id: String,
    compatibility_fingerprint: String,
    credential_id: String,
    recovery_point_id: String,
    confirmation_token: String,
}

impl LogicalPrunePlan {
    pub(crate) fn new(options: LogicalPrunePlanOptions<'_>) -> Result<Self, String> {
        validate_request(&options)?;
        let (logical, strategy) = one_logical(&options)?;
        let credential = one_credential(&options)?;
        let recovery = one_recovery_point(&options, logical)?;
        let confirmation_token =
            confirmation_token(options.installation_id, logical, credential, recovery);

        Ok(Self {
            strategy,
            project_id: logical.project_id().to_owned(),
            service_id: logical.service_id().to_owned(),
            logical_resource_id: logical.logical_resource_id().to_owned(),
            shared_resource_id: logical.shared_resource_id().to_owned(),
            compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
            credential_id: credential.credential_id().to_owned(),
            recovery_point_id: recovery.recovery_point_id().to_owned(),
            confirmation_token,
        })
    }

    pub(crate) const fn strategy(&self) -> DataLifecycleStrategy {
        self.strategy
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn logical_resource_id(&self) -> &str {
        &self.logical_resource_id
    }

    pub(crate) fn shared_resource_id(&self) -> &str {
        &self.shared_resource_id
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }

    pub(crate) fn credential_id(&self) -> &str {
        &self.credential_id
    }

    pub(crate) fn recovery_point_id(&self) -> &str {
        &self.recovery_point_id
    }

    pub(crate) fn confirmation_token(&self) -> &str {
        &self.confirmation_token
    }
}

fn validate_request(options: &LogicalPrunePlanOptions<'_>) -> Result<(), String> {
    if options.installation_id.is_empty()
        || options.project_id.is_empty()
        || options.service_id.is_empty()
        || options.recovery_point_id.is_empty()
    {
        return Err("logical prune identity must not be empty".to_owned());
    }
    if options.project_registered {
        return Err(format!(
            "project '{}' is still registered; remove its configuration and wait for orphaning before prune",
            options.project_id
        ));
    }

    Ok(())
}

fn one_logical<'state>(
    options: &LogicalPrunePlanOptions<'state>,
) -> Result<(&'state LogicalResourceRecord, DataLifecycleStrategy), String> {
    let matches = options
        .logical_resources
        .iter()
        .filter(|logical| {
            logical.project_id() == options.project_id && logical.service_id() == options.service_id
        })
        .collect::<Vec<_>>();
    let [logical] = matches.as_slice() else {
        return Err(format!(
            "project '{}/{}' must have exactly one retained logical resource; found {}",
            options.project_id,
            options.service_id,
            matches.len()
        ));
    };
    let strategy = resolve_data_lifecycle_strategy(logical).map_err(|error| error.to_string())?;
    if !matches!(
        strategy,
        DataLifecycleStrategy::PostgreSqlLogical
            | DataLifecycleStrategy::MySqlLogical
            | DataLifecycleStrategy::MongoDbLogical
            | DataLifecycleStrategy::SqlServerNative
    ) {
        return Err(format!(
            "logical resource kind '{}' has no implemented destructive prune adapter",
            logical.kind()
        ));
    }
    if logical.lifecycle() == ResourceLifecycle::Active
        || logical.orphaned_at_unix_seconds().is_none()
    {
        return Err(format!(
            "logical resource '{}' must be orphaned before destructive prune",
            logical.logical_resource_id()
        ));
    }

    Ok((logical, strategy))
}

fn one_credential<'state>(
    options: &LogicalPrunePlanOptions<'state>,
) -> Result<&'state CredentialRecord, String> {
    let matches = options
        .credentials
        .iter()
        .filter(|credential| {
            credential.project_id() == Some(options.project_id)
                && credential.service_id() == options.service_id
        })
        .collect::<Vec<_>>();
    let [credential] = matches.as_slice() else {
        return Err(format!(
            "project '{}/{}' must have exactly one retained credential; found {}",
            options.project_id,
            options.service_id,
            matches.len()
        ));
    };
    if credential.lifecycle() != CredentialLifecycle::Disabled {
        return Err(format!(
            "credential '{}' must be disabled before destructive prune",
            credential.credential_id()
        ));
    }

    Ok(credential)
}

fn one_recovery_point<'state>(
    options: &LogicalPrunePlanOptions<'state>,
    logical: &LogicalResourceRecord,
) -> Result<&'state RecoveryPointRecord, String> {
    let matches = options
        .recovery_points
        .iter()
        .filter(|recovery| recovery.recovery_point_id() == options.recovery_point_id)
        .collect::<Vec<_>>();
    let [recovery] = matches.as_slice() else {
        return Err(format!(
            "recovery point '{}' must identify exactly one verified artifact; found {}",
            options.recovery_point_id,
            matches.len()
        ));
    };
    let exact = recovery.project_id() == logical.project_id()
        && recovery.service_id() == logical.service_id()
        && recovery.logical_resource_id() == logical.logical_resource_id()
        && recovery.resource_kind() == logical.kind()
        && recovery.compatibility_fingerprint() == logical.compatibility_fingerprint();
    if !exact {
        return Err(format!(
            "recovery point '{}' does not protect the exact retained logical resource",
            recovery.recovery_point_id()
        ));
    }

    Ok(recovery)
}

fn confirmation_token(
    installation_id: &str,
    logical: &LogicalResourceRecord,
    credential: &CredentialRecord,
    recovery: &RecoveryPointRecord,
) -> String {
    let mut hasher = Sha256::new();
    for field in [
        "stackctl-logical-prune-v1",
        installation_id,
        logical.project_id(),
        logical.service_id(),
        logical.logical_resource_id(),
        logical.shared_resource_id(),
        logical.kind(),
        logical.compatibility_fingerprint(),
        logical.desired_revision(),
        credential.credential_id(),
        credential.username(),
        recovery.recovery_point_id(),
        recovery.reference(),
        recovery.artifact_sha256(),
    ] {
        hash_field(&mut hasher, field.as_bytes());
    }
    for number in [
        logical.orphaned_at_unix_seconds().unwrap_or_default(),
        recovery.created_at_unix_seconds(),
        recovery.verified_at_unix_seconds(),
    ] {
        hash_field(&mut hasher, &number.to_be_bytes());
    }
    hash_field(&mut hasher, &recovery.artifact_size_bytes().to_be_bytes());

    hex::encode(hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, field: &[u8]) {
    hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(field);
}
