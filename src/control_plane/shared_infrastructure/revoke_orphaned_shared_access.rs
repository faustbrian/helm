use super::{
    CredentialSecret, MongoDbAccessRevocationOptions, MySqlAccessRevocationOptions, MySqlFlavor,
    OrphanedSharedAccessOptions, PostgresAccessRevocationOptions, RabbitMqProjectDefinition,
    RedisAccessRevocationOptions, RedisFlavor, SharedInfrastructureReconcileError,
    SqlServerAccessRevocationOptions, revoke_mongodb_project_access, revoke_mysql_project_access,
    revoke_postgres_project_access, revoke_rabbitmq_project_access, revoke_redis_project_access,
    revoke_sql_server_project_access,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, ContainerState, EngineError,
    ManagedResourceMetadata, ObservedResourceOwnership, OwnedContainer, ResourceKind,
    RetentionClass, reconstruct_owned_container,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, LogicalResourceRecord, ResourceLifecycle,
    ResourceRecord, ResourceRetention,
};

#[derive(Clone, Copy)]
enum AccessStrategy {
    MongoDb,
    MySql(MySqlFlavor),
    Postgres,
    RabbitMq,
    Redis(RedisFlavor),
    SqlServer,
}

/// Revokes orphaned tenant users without deleting their retained logical data.
pub(crate) async fn revoke_orphaned_shared_access<E>(
    engine: &mut E,
    options: OrphanedSharedAccessOptions<'_>,
) -> Result<usize, SharedInfrastructureReconcileError>
where
    E: CommandExecutor + ContainerDiscovery + ContainerLifecycle,
{
    if options.timeout.is_zero() {
        return Err(conflict(
            "shared access revocation timeout must be positive",
        ));
    }
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("discover shared access targets", error))?;
    let mut shared = Vec::new();
    for container in &observed {
        match reconstruct_owned_container(
            container,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(owned) if owned.metadata().kind() == ResourceKind::SharedService => {
                shared.push(owned);
            }
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => {}
            Err(ownership) => {
                return Err(conflict(format!(
                    "managed container '{}' has invalid ownership while revoking shared access: {ownership:?}",
                    container.id().as_str()
                )));
            }
        }
    }

    let mut revoked = 0;
    for logical in options
        .logical_resources
        .iter()
        .filter(|logical| logical.lifecycle() == ResourceLifecycle::Orphaned)
    {
        let Some(strategy) = access_strategy(logical.kind()) else {
            continue;
        };
        let credential = exact_project_credential(logical, strategy, options.credentials)?;
        let candidates = shared
            .iter()
            .filter(|container| {
                container.metadata().compatibility_fingerprint()
                    == logical.compatibility_fingerprint()
            })
            .collect::<Vec<_>>();
        let Some(container) = exact_candidate(logical, &candidates)? else {
            continue;
        };
        let resource = options
            .resources
            .iter()
            .find(|resource| resource.resource_id() == container.id().as_str())
            .ok_or_else(|| {
                conflict(format!(
                    "shared service '{}' has no durable ownership record",
                    container.id().as_str()
                ))
            })?;
        if !matches_durable_ownership(resource, container.metadata()) {
            return Err(conflict(format!(
                "shared service '{}' differs from its durable ownership record",
                container.id().as_str()
            )));
        }
        match engine
            .inspect(container)
            .await
            .map_err(|error| engine_error("inspect shared access target", error))?
        {
            ContainerState::Running => {}
            ContainerState::Stopped => engine
                .start(container)
                .await
                .map_err(|error| engine_error("start shared access target", error))?,
            ContainerState::Missing => continue,
        }

        let changed = match strategy {
            AccessStrategy::MongoDb => {
                let administrator = mongodb_administrator(logical, options.credentials)?;
                revoke_mongodb_project_access(
                    engine,
                    MongoDbAccessRevocationOptions {
                        installation_id: options.installation_id,
                        container,
                        logical_resource: logical,
                        credential,
                        administrator,
                        timeout: options.timeout,
                    },
                )
                .await
                .map_err(|error| engine_error("revoke orphaned MongoDB access", error))?
            }
            AccessStrategy::MySql(flavor) => {
                let administrator = mysql_administrator(logical, flavor, options.credentials)?;
                revoke_mysql_project_access(
                    engine,
                    MySqlAccessRevocationOptions {
                        installation_id: options.installation_id,
                        container,
                        logical_resource: logical,
                        credential,
                        administrator,
                        flavor,
                        timeout: options.timeout,
                    },
                )
                .await
                .map_err(|error| engine_error("revoke orphaned MySQL-family access", error))?
            }
            AccessStrategy::Postgres => {
                let administrator = postgres_administrator(logical, options.credentials)?;
                revoke_postgres_project_access(
                    engine,
                    PostgresAccessRevocationOptions {
                        installation_id: options.installation_id,
                        container,
                        logical_resource: logical,
                        credential,
                        administrator,
                        timeout: options.timeout,
                    },
                )
                .await
                .map_err(|error| engine_error("revoke orphaned PostgreSQL access", error))?
            }
            AccessStrategy::RabbitMq => {
                let definition = RabbitMqProjectDefinition::new(
                    logical.project_id(),
                    logical.service_id(),
                    CredentialSecret::new(credential.secret().to_owned()),
                )
                .map_err(|error| conflict(error.to_string()))?;
                if definition.username() != credential.username() {
                    return Err(conflict(format!(
                        "orphaned RabbitMQ tenant '{}' credential identity does not match its deterministic project user",
                        logical.logical_resource_id()
                    )));
                }
                revoke_rabbitmq_project_access(engine, container, &definition)
                    .await
                    .map_err(|error| engine_error("revoke orphaned RabbitMQ access", error))?
            }
            AccessStrategy::Redis(flavor) => {
                let administrator = redis_administrator(logical, flavor, options.credentials)?;
                revoke_redis_project_access(
                    engine,
                    RedisAccessRevocationOptions {
                        installation_id: options.installation_id,
                        container,
                        logical_resource: logical,
                        credential,
                        administrator,
                        flavor,
                        timeout: options.timeout,
                    },
                )
                .await
                .map_err(|error| engine_error("revoke orphaned Redis-compatible access", error))?
            }
            AccessStrategy::SqlServer => {
                let administrator = sql_server_administrator(logical, options.credentials)?;
                revoke_sql_server_project_access(
                    engine,
                    SqlServerAccessRevocationOptions {
                        installation_id: options.installation_id,
                        container,
                        logical_resource: logical,
                        credential,
                        administrator,
                        timeout: options.timeout,
                    },
                )
                .await
                .map_err(|error| engine_error("revoke orphaned SQL Server access", error))?
            }
        };
        if changed {
            revoked += 1;
        }
    }

    Ok(revoked)
}

fn access_strategy(kind: &str) -> Option<AccessStrategy> {
    match kind {
        "mongodb_database" => Some(AccessStrategy::MongoDb),
        "mariadb_database" => Some(AccessStrategy::MySql(MySqlFlavor::MariaDb)),
        "mysql_database" => Some(AccessStrategy::MySql(MySqlFlavor::MySql)),
        "postgres_database_and_role" => Some(AccessStrategy::Postgres),
        "rabbitmq_vhost_user" => Some(AccessStrategy::RabbitMq),
        "redis_acl_prefix" => Some(AccessStrategy::Redis(RedisFlavor::Redis)),
        "sqlserver_database" => Some(AccessStrategy::SqlServer),
        "valkey_acl_prefix" => Some(AccessStrategy::Redis(RedisFlavor::Valkey)),
        _ => None,
    }
}

fn exact_project_credential<'credential>(
    logical: &LogicalResourceRecord,
    strategy: AccessStrategy,
    credentials: &'credential [CredentialRecord],
) -> Result<&'credential CredentialRecord, SharedInfrastructureReconcileError> {
    let credential_id = match strategy {
        AccessStrategy::Postgres => format!(
            "{}/{}/postgresql",
            logical.project_id(),
            logical.service_id()
        ),
        AccessStrategy::MongoDb
        | AccessStrategy::MySql(_)
        | AccessStrategy::RabbitMq
        | AccessStrategy::Redis(_)
        | AccessStrategy::SqlServer => logical.logical_resource_id().to_owned(),
    };
    let credential = credentials
        .iter()
        .find(|credential| credential.credential_id() == credential_id)
        .ok_or_else(|| {
            conflict(format!(
                "orphaned shared tenant '{}' has no retained credential",
                logical.logical_resource_id()
            ))
        })?;
    if credential.lifecycle() != CredentialLifecycle::Disabled
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
    {
        return Err(conflict(format!(
            "orphaned shared tenant '{}' requires its exact disabled project credential",
            logical.logical_resource_id()
        )));
    }

    Ok(credential)
}

fn mongodb_administrator<'credential>(
    logical: &LogicalResourceRecord,
    credentials: &'credential [CredentialRecord],
) -> Result<&'credential CredentialRecord, SharedInfrastructureReconcileError> {
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let credential_id = format!("shared/{fingerprint}/mongodb-bootstrap");
    credentials
        .iter()
        .find(|credential| credential.credential_id() == credential_id)
        .ok_or_else(|| {
            conflict(format!(
                "orphaned MongoDB tenant '{}' has no retained administrator credential",
                logical.logical_resource_id()
            ))
        })
}

fn mysql_administrator<'credential>(
    logical: &LogicalResourceRecord,
    flavor: MySqlFlavor,
    credentials: &'credential [CredentialRecord],
) -> Result<&'credential CredentialRecord, SharedInfrastructureReconcileError> {
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let credential_id = format!("shared/{fingerprint}/{}-bootstrap", flavor.implementation());
    credentials
        .iter()
        .find(|credential| credential.credential_id() == credential_id)
        .ok_or_else(|| {
            conflict(format!(
                "orphaned {} tenant '{}' has no retained administrator credential",
                flavor.implementation(),
                logical.logical_resource_id()
            ))
        })
}

fn postgres_administrator<'credential>(
    logical: &LogicalResourceRecord,
    credentials: &'credential [CredentialRecord],
) -> Result<&'credential CredentialRecord, SharedInfrastructureReconcileError> {
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let credential_id = format!("shared/{fingerprint}/postgresql-bootstrap");
    credentials
        .iter()
        .find(|credential| credential.credential_id() == credential_id)
        .ok_or_else(|| {
            conflict(format!(
                "orphaned PostgreSQL tenant '{}' has no retained administrator credential",
                logical.logical_resource_id()
            ))
        })
}

fn redis_administrator<'credential>(
    logical: &LogicalResourceRecord,
    flavor: RedisFlavor,
    credentials: &'credential [CredentialRecord],
) -> Result<&'credential CredentialRecord, SharedInfrastructureReconcileError> {
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let credential_id = format!("shared/{fingerprint}/{}-bootstrap", flavor.implementation());
    credentials
        .iter()
        .find(|credential| credential.credential_id() == credential_id)
        .ok_or_else(|| {
            conflict(format!(
                "orphaned {} tenant '{}' has no retained administrator credential",
                flavor.implementation(),
                logical.logical_resource_id()
            ))
        })
}

fn sql_server_administrator<'credential>(
    logical: &LogicalResourceRecord,
    credentials: &'credential [CredentialRecord],
) -> Result<&'credential CredentialRecord, SharedInfrastructureReconcileError> {
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let credential_id = format!("shared/{fingerprint}/sqlserver-bootstrap");
    credentials
        .iter()
        .find(|credential| credential.credential_id() == credential_id)
        .ok_or_else(|| {
            conflict(format!(
                "orphaned SQL Server tenant '{}' has no retained administrator credential",
                logical.logical_resource_id()
            ))
        })
}

fn exact_candidate<'container>(
    logical: &LogicalResourceRecord,
    candidates: &[&'container OwnedContainer],
) -> Result<Option<&'container OwnedContainer>, SharedInfrastructureReconcileError> {
    match candidates {
        [] => Ok(None),
        [container] => Ok(Some(*container)),
        _ => Err(conflict(format!(
            "orphaned shared tenant '{}' matches multiple shared services",
            logical.logical_resource_id()
        ))),
    }
}

fn matches_durable_ownership(
    resource: &ResourceRecord,
    metadata: &ManagedResourceMetadata,
) -> bool {
    resource.installation_id() == metadata.installation_id()
        && resource.kind() == metadata.kind().label()
        && resource.scope_id() == metadata.resource_id()
        && resource.compatibility_fingerprint() == metadata.compatibility_fingerprint()
        && resource.project_id() == metadata.project_id()
        && resource.schema_version() == metadata.schema_version()
        && resource.desired_revision() == metadata.desired_revision()
        && resource.retention() == retention(metadata.retention())
        && resource.lifecycle() == ResourceLifecycle::Active
}

const fn retention(retention: RetentionClass) -> ResourceRetention {
    match retention {
        RetentionClass::Persistent => ResourceRetention::Persistent,
        RetentionClass::Disposable => ResourceRetention::Disposable,
        RetentionClass::BuildCache => ResourceRetention::BuildCache,
    }
}

fn conflict(detail: impl Into<String>) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Conflict {
        detail: detail.into(),
    }
}

fn engine_error(action: &str, error: EngineError) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
