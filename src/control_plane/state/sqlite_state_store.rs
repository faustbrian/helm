use super::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
    EnvironmentLifecycle, InstallationRecord, ManagedEnvironmentRecord,
    ManagedEnvironmentRecordOptions, ProjectAdoptionPlan, ProjectRecord, ResourceLifecycle,
    ResourceRecord, ResourceRecordOptions, ResourceRetention, StateStore, StateStoreError,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const CURRENT_SCHEMA_VERSION: u32 = 6;

/// The bundled-SQLite adapter for durable per-user control-plane state.
pub(crate) struct SqliteStateStore {
    pub(super) connection: Connection,
}

impl SqliteStateStore {
    /// Opens a store and atomically applies every supported migration.
    pub(crate) fn open(database_path: &Path) -> Result<Self, StateStoreError> {
        let connection = Connection::open(database_path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;\n\
             PRAGMA journal_mode = WAL;\n\
             PRAGMA synchronous = NORMAL;\n\
             PRAGMA busy_timeout = 5000;",
        )?;
        let mut store = Self { connection };
        store.migrate()?;

        Ok(store)
    }

    /// Returns the durable schema version.
    pub(crate) fn schema_version(&self) -> Result<u32, StateStoreError> {
        self.connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(Into::into)
    }

    /// Returns the active journal mode for operational diagnostics.
    pub(crate) fn journal_mode(&self) -> Result<String, StateStoreError> {
        self.connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(Into::into)
    }

    fn migrate(&mut self) -> Result<(), StateStoreError> {
        let found = self.schema_version()?;

        if found > CURRENT_SCHEMA_VERSION {
            return Err(StateStoreError::UnsupportedSchema {
                found,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }

        if found == CURRENT_SCHEMA_VERSION {
            return Ok(());
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if found < 1 {
            transaction.execute_batch(
                "CREATE TABLE projects (\n\
                 canonical_path TEXT PRIMARY KEY NOT NULL,\n\
                 project_name TEXT NOT NULL\n\
             ) STRICT;\n\
             CREATE TABLE route_claims (\n\
                 domain TEXT PRIMARY KEY NOT NULL,\n\
                 canonical_path TEXT NOT NULL\n\
                     REFERENCES projects(canonical_path) ON DELETE CASCADE\n\
             ) STRICT;\n\
             CREATE INDEX route_claims_project_idx\n\
                 ON route_claims(canonical_path);",
            )?;
        }

        if found < 2 {
            transaction.execute_batch(
                "CREATE TABLE resources (\n\
                     resource_id TEXT PRIMARY KEY NOT NULL,\n\
                     installation_id TEXT NOT NULL,\n\
                     kind TEXT NOT NULL,\n\
                     compatibility_fingerprint TEXT NOT NULL,\n\
                     project_id TEXT,\n\
                     resource_schema_version INTEGER NOT NULL CHECK(resource_schema_version > 0),\n\
                     desired_revision TEXT NOT NULL,\n\
                     retention TEXT NOT NULL CHECK(retention IN ('persistent', 'disposable', 'build_cache')),\n\
                     lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active', 'orphaned', 'retained')),\n\
                     orphaned_at_unix_seconds INTEGER\n\
                 ) STRICT;\n\
                 CREATE INDEX resources_project_idx ON resources(project_id);\n\
                 CREATE INDEX resources_fingerprint_idx\n\
                     ON resources(compatibility_fingerprint);",
            )?;
        }

        if found < 3 {
            transaction.execute_batch(
                "CREATE TABLE installation (\n\
                     singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),\n\
                     installation_id TEXT NOT NULL CHECK(length(installation_id) > 0),\n\
                     engine_provider TEXT NOT NULL CHECK(engine_provider IN ('docker')),\n\
                     engine_endpoint TEXT NOT NULL CHECK(length(engine_endpoint) > 0)\n\
                 ) STRICT;\n\
                 CREATE TABLE watched_roots (\n\
                     canonical_path TEXT PRIMARY KEY NOT NULL\n\
                 ) STRICT;",
            )?;
        }

        if found < 4 {
            transaction.execute_batch(
                "CREATE TABLE credentials (\n\
                     credential_id TEXT PRIMARY KEY NOT NULL,\n\
                     project_id TEXT NOT NULL CHECK(length(project_id) > 0),\n\
                     service_id TEXT NOT NULL CHECK(length(service_id) > 0),\n\
                     username TEXT NOT NULL CHECK(length(username) > 0),\n\
                     secret TEXT NOT NULL CHECK(length(secret) > 0),\n\
                     lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active', 'disabled'))\n\
                 ) STRICT;\n\
                 CREATE INDEX credentials_project_idx ON credentials(project_id);",
            )?;
        }

        if found < 5 {
            transaction.execute_batch(
                "CREATE TABLE managed_environments (\n\
                     project_id TEXT PRIMARY KEY NOT NULL CHECK(length(project_id) > 0),\n\
                     revision TEXT NOT NULL CHECK(length(revision) > 0),\n\
                     values_json TEXT NOT NULL,\n\
                     lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active', 'disabled'))\n\
                 ) STRICT;",
            )?;
        }

        if found < 6 {
            transaction.execute_batch(
                "ALTER TABLE credentials RENAME TO credentials_v5;\n\
                 DROP INDEX credentials_project_idx;\n\
                 CREATE TABLE credentials (\n\
                     credential_id TEXT PRIMARY KEY NOT NULL,\n\
                     project_id TEXT,\n\
                     service_id TEXT NOT NULL CHECK(length(service_id) > 0),\n\
                     username TEXT NOT NULL CHECK(length(username) > 0),\n\
                     secret TEXT NOT NULL CHECK(length(secret) > 0),\n\
                     lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active', 'disabled'))\n\
                 ) STRICT;\n\
                 INSERT INTO credentials\n\
                     (credential_id, project_id, service_id, username, secret, lifecycle)\n\
                 SELECT credential_id, project_id, service_id, username, secret, lifecycle\n\
                 FROM credentials_v5;\n\
                 DROP TABLE credentials_v5;\n\
                 CREATE INDEX credentials_project_idx ON credentials(project_id);",
            )?;
        }

        transaction.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
        transaction.commit()?;

        Ok(())
    }
}

impl StateStore for SqliteStateStore {
    fn initialize_installation(
        &mut self,
        installation: &InstallationRecord,
    ) -> Result<(), StateStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_installation(&transaction)?;

        if let Some(existing) = existing {
            if existing != *installation {
                return Err(StateStoreError::InstallationAlreadyInitialized {
                    existing_installation_id: existing.installation_id().to_owned(),
                });
            }
        } else {
            transaction.execute(
                "INSERT INTO installation (\n\
                     singleton, installation_id, engine_provider, engine_endpoint\n\
                 ) VALUES (1, ?1, ?2, ?3)",
                params![
                    installation.installation_id(),
                    installation.engine_provider().label(),
                    installation.engine_endpoint(),
                ],
            )?;
        }

        transaction.commit()?;

        Ok(())
    }

    fn installation(&self) -> Result<Option<InstallationRecord>, StateStoreError> {
        load_installation(&self.connection)
    }

    fn replace_watched_roots(&mut self, roots: &[PathBuf]) -> Result<(), StateStoreError> {
        let roots = roots
            .iter()
            .map(|root| exact_path(root))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        transaction.execute("DELETE FROM watched_roots", [])?;
        for root in roots {
            transaction.execute(
                "INSERT INTO watched_roots (canonical_path) VALUES (?1)",
                [root],
            )?;
        }

        transaction.commit()?;

        Ok(())
    }

    fn watched_roots(&self) -> Result<Vec<PathBuf>, StateStoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT canonical_path FROM watched_roots ORDER BY canonical_path")?;
        let roots = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(roots.into_iter().map(PathBuf::from).collect())
    }

    fn replace_projects(&mut self, projects: &[ProjectRecord]) -> Result<(), StateStoreError> {
        let exact_projects = projects
            .iter()
            .map(|project| exact_path(project.canonical_path()).map(|path| (project, path)))
            .collect::<Result<Vec<_>, _>>()?;
        let batch_paths = exact_projects
            .iter()
            .map(|(_, path)| *path)
            .collect::<BTreeSet<_>>();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        for (project, canonical_path) in &exact_projects {
            for domain in project.route_domains() {
                let existing_path = transaction
                    .query_row(
                        "SELECT canonical_path FROM route_claims WHERE domain = ?1",
                        [domain],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;

                if let Some(existing_path) = existing_path {
                    if existing_path != *canonical_path
                        && !batch_paths.contains(existing_path.as_str())
                    {
                        return Err(StateStoreError::RouteOwnershipConflict {
                            domain: domain.clone(),
                            existing_path: PathBuf::from(existing_path),
                            requested_path: project.canonical_path().to_path_buf(),
                        });
                    }
                }
            }
        }

        for (project, canonical_path) in &exact_projects {
            transaction.execute(
                "INSERT INTO projects (canonical_path, project_name) VALUES (?1, ?2)\n\
                 ON CONFLICT(canonical_path) DO UPDATE SET project_name = excluded.project_name",
                params![canonical_path, project.project_name()],
            )?;
            transaction.execute(
                "DELETE FROM route_claims WHERE canonical_path = ?1",
                [canonical_path],
            )?;
        }

        for (project, canonical_path) in exact_projects {
            for domain in project.route_domains() {
                transaction.execute(
                    "INSERT INTO route_claims (domain, canonical_path) VALUES (?1, ?2)",
                    params![domain, canonical_path],
                )?;
            }
        }

        transaction.commit()?;

        Ok(())
    }

    fn projects(&self) -> Result<Vec<ProjectRecord>, StateStoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT canonical_path, project_name FROM projects ORDER BY canonical_path")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let projects = rows.collect::<Result<Vec<_>, _>>()?;
        let mut records = Vec::with_capacity(projects.len());

        for (canonical_path, project_name) in projects {
            let mut route_statement = self.connection.prepare(
                "SELECT domain FROM route_claims\n\
                 WHERE canonical_path = ?1 ORDER BY domain",
            )?;
            let routes = route_statement
                .query_map([&canonical_path], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;

            records.push(ProjectRecord::new(
                PathBuf::from(canonical_path),
                project_name,
                routes,
            ));
        }

        Ok(records)
    }

    fn orphan_project(
        &mut self,
        canonical_path: &Path,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError> {
        let canonical_path = exact_path(canonical_path)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let project_id = transaction
            .query_row(
                "SELECT project_name FROM projects WHERE canonical_path = ?1",
                [canonical_path],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        if let Some(project_id) = project_id {
            transaction.execute(
                "UPDATE resources
                 SET lifecycle = 'orphaned', orphaned_at_unix_seconds = ?1
                 WHERE project_id = ?2 AND lifecycle = 'active'",
                params![orphaned_at_unix_seconds, project_id],
            )?;
            transaction.execute(
                "UPDATE credentials SET lifecycle = 'disabled'\n\
                 WHERE project_id = ?1 AND lifecycle = 'active'",
                [&project_id],
            )?;
            transaction.execute(
                "UPDATE managed_environments SET lifecycle = 'disabled'\n\
                 WHERE project_id = ?1 AND lifecycle = 'active'",
                [&project_id],
            )?;
            transaction.execute(
                "DELETE FROM projects WHERE canonical_path = ?1",
                [canonical_path],
            )?;
        }

        transaction.commit()?;

        Ok(())
    }

    fn upsert_resources(&mut self, resources: &[ResourceRecord]) -> Result<(), StateStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        for resource in resources {
            if let Some(existing) = load_resource_ownership(&transaction, resource.resource_id())? {
                if !existing.matches(resource) {
                    return Err(StateStoreError::ResourceOwnershipConflict {
                        resource_id: resource.resource_id().to_owned(),
                    });
                }
                if !existing.is_active() && resource.lifecycle() == ResourceLifecycle::Active {
                    return Err(StateStoreError::ResourceAdoptionRequired {
                        resource_id: resource.resource_id().to_owned(),
                    });
                }
            }
            transaction.execute(
                "INSERT INTO resources (\n\
                     resource_id, installation_id, kind, compatibility_fingerprint,\n\
                     project_id, resource_schema_version, desired_revision, retention,\n\
                     lifecycle, orphaned_at_unix_seconds\n\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)\n\
                 ON CONFLICT(resource_id) DO UPDATE SET\n\
                     desired_revision = excluded.desired_revision,\n\
                     lifecycle = excluded.lifecycle,\n\
                     orphaned_at_unix_seconds = excluded.orphaned_at_unix_seconds",
                params![
                    resource.resource_id(),
                    resource.installation_id(),
                    resource.kind(),
                    resource.compatibility_fingerprint(),
                    resource.project_id(),
                    resource.schema_version(),
                    resource.desired_revision(),
                    resource.retention().label(),
                    resource.lifecycle().label(),
                    resource.orphaned_at_unix_seconds(),
                ],
            )?;
        }

        transaction.commit()?;

        Ok(())
    }

    fn adopt_project(&mut self, adoption: &ProjectAdoptionPlan) -> Result<(), StateStoreError> {
        let canonical_path = exact_path(adoption.canonical_path())?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let registered_project = transaction
            .query_row(
                "SELECT project_name FROM projects WHERE canonical_path = ?1",
                [canonical_path],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if registered_project.as_deref() != Some(adoption.project_id()) {
            return Err(StateStoreError::ProjectAdoptionTargetMissing {
                project_id: adoption.project_id().to_owned(),
                path: adoption.canonical_path().to_path_buf(),
            });
        }

        for resource in adoption.resources() {
            let existing = load_resource_ownership(&transaction, resource.resource_id())?
                .ok_or_else(|| StateStoreError::ProjectAdoptionStateMismatch {
                    project_id: adoption.project_id().to_owned(),
                    detail: format!("resource '{}' is missing", resource.resource_id()),
                })?;
            if !existing.matches(resource) {
                return Err(StateStoreError::ResourceOwnershipConflict {
                    resource_id: resource.resource_id().to_owned(),
                });
            }
        }
        for credential_id in adoption.credential_ids() {
            let owner = transaction
                .query_row(
                    "SELECT project_id FROM credentials WHERE credential_id = ?1",
                    [credential_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;
            if owner.flatten().as_deref() != Some(adoption.project_id()) {
                return Err(StateStoreError::ProjectAdoptionStateMismatch {
                    project_id: adoption.project_id().to_owned(),
                    detail: format!(
                        "credential '{credential_id}' is missing or owned by another project"
                    ),
                });
            }
        }
        let environment_revision = transaction
            .query_row(
                "SELECT revision FROM managed_environments WHERE project_id = ?1",
                [adoption.project_id()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if environment_revision.as_deref() != Some(adoption.environment_revision()) {
            return Err(StateStoreError::ProjectAdoptionStateMismatch {
                project_id: adoption.project_id().to_owned(),
                detail: format!(
                    "managed environment revision '{}' is not retained",
                    adoption.environment_revision()
                ),
            });
        }

        for resource in adoption.resources() {
            transaction.execute(
                "UPDATE resources
                 SET desired_revision = ?1, lifecycle = 'active',
                     orphaned_at_unix_seconds = NULL
                 WHERE resource_id = ?2",
                params![resource.desired_revision(), resource.resource_id()],
            )?;
        }
        for credential_id in adoption.credential_ids() {
            transaction.execute(
                "UPDATE credentials SET lifecycle = 'active' WHERE credential_id = ?1",
                [credential_id],
            )?;
        }
        transaction.execute(
            "UPDATE managed_environments SET lifecycle = 'active' WHERE project_id = ?1",
            [adoption.project_id()],
        )?;
        transaction.commit()?;

        Ok(())
    }

    fn resources(&self) -> Result<Vec<ResourceRecord>, StateStoreError> {
        let mut statement = self.connection.prepare(
            "SELECT resource_id, installation_id, kind, compatibility_fingerprint,\n\
                    project_id, resource_schema_version, desired_revision, retention,\n\
                    lifecycle, orphaned_at_unix_seconds\n\
             FROM resources ORDER BY resource_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<i64>>(9)?,
            ))
        })?;
        let persisted = rows.collect::<Result<Vec<_>, _>>()?;

        persisted
            .into_iter()
            .map(
                |(
                    resource_id,
                    installation_id,
                    kind,
                    compatibility_fingerprint,
                    project_id,
                    schema_version,
                    desired_revision,
                    retention,
                    lifecycle,
                    orphaned_at_unix_seconds,
                )| {
                    let schema_version = u32::try_from(schema_version).map_err(|_| {
                        StateStoreError::CorruptState {
                            detail: format!(
                                "resource '{resource_id}' has invalid schema version {schema_version}"
                            ),
                        }
                    })?;
                    let retention = ResourceRetention::from_label(&retention).ok_or_else(|| {
                        StateStoreError::CorruptState {
                            detail: format!(
                                "resource '{resource_id}' has unknown retention '{retention}'"
                            ),
                        }
                    })?;
                    let lifecycle = ResourceLifecycle::from_label(&lifecycle).ok_or_else(|| {
                        StateStoreError::CorruptState {
                            detail: format!(
                                "resource '{resource_id}' has unknown lifecycle '{lifecycle}'"
                            ),
                        }
                    })?;

                    Ok(ResourceRecord::new(ResourceRecordOptions {
                        resource_id,
                        installation_id,
                        kind,
                        compatibility_fingerprint,
                        project_id,
                        schema_version,
                        desired_revision,
                        retention,
                        lifecycle,
                        orphaned_at_unix_seconds,
                    }))
                },
            )
            .collect()
    }

    fn insert_credential_if_absent(
        &mut self,
        credential: &CredentialRecord,
    ) -> Result<CredentialRecord, StateStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_credential(&transaction, credential.credential_id())?;

        if let Some(existing) = existing {
            if existing.project_id() != credential.project_id()
                || existing.service_id() != credential.service_id()
                || existing.username() != credential.username()
            {
                return Err(StateStoreError::CredentialOwnershipConflict {
                    credential_id: credential.credential_id().to_owned(),
                });
            }

            transaction.commit()?;
            return Ok(existing);
        }

        transaction.execute(
            "INSERT INTO credentials (\n\
                 credential_id, project_id, service_id, username, secret, lifecycle\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                credential.credential_id(),
                credential.project_id(),
                credential.service_id(),
                credential.username(),
                credential.secret(),
                credential.lifecycle().label(),
            ],
        )?;
        transaction.commit()?;

        Ok(credential.clone())
    }

    fn credentials(&self) -> Result<Vec<CredentialRecord>, StateStoreError> {
        let mut statement = self.connection.prepare(
            "SELECT credential_id, project_id, service_id, username, secret, lifecycle\n\
             FROM credentials ORDER BY credential_id",
        )?;
        let persisted = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        persisted
            .into_iter()
            .map(credential_from_persisted)
            .collect()
    }

    fn replace_managed_environment(
        &mut self,
        environment: &ManagedEnvironmentRecord,
    ) -> Result<(), StateStoreError> {
        let values_json = serde_json::to_string(environment.values()).map_err(|error| {
            StateStoreError::CorruptState {
                detail: format!("failed to encode managed environment: {error}"),
            }
        })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing_lifecycle = transaction
            .query_row(
                "SELECT lifecycle FROM managed_environments WHERE project_id = ?1",
                [environment.project_id()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if existing_lifecycle.as_deref() == Some(EnvironmentLifecycle::Disabled.label())
            && environment.lifecycle() == EnvironmentLifecycle::Active
        {
            return Err(StateStoreError::ProjectAdoptionRequired {
                project_id: environment.project_id().to_owned(),
            });
        }
        transaction.execute(
            "INSERT INTO managed_environments (\n\
                 project_id, revision, values_json, lifecycle\n\
             ) VALUES (?1, ?2, ?3, ?4)\n\
             ON CONFLICT(project_id) DO UPDATE SET\n\
                 revision = excluded.revision,\n\
                 values_json = excluded.values_json,\n\
                 lifecycle = excluded.lifecycle",
            params![
                environment.project_id(),
                environment.revision(),
                values_json,
                environment.lifecycle().label(),
            ],
        )?;
        transaction.commit()?;

        Ok(())
    }

    fn managed_environments(&self) -> Result<Vec<ManagedEnvironmentRecord>, StateStoreError> {
        let mut statement = self.connection.prepare(
            "SELECT project_id, revision, values_json, lifecycle\n\
             FROM managed_environments ORDER BY project_id",
        )?;
        let persisted = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        persisted
            .into_iter()
            .map(
                |(project_id, revision, values_json, lifecycle)| -> Result<_, StateStoreError> {
                    let values = serde_json::from_str::<BTreeMap<String, String>>(&values_json)
                        .map_err(|error| StateStoreError::CorruptState {
                            detail: format!(
                                "managed environment '{project_id}' has invalid values: {error}"
                            ),
                        })?;
                    let lifecycle =
                        EnvironmentLifecycle::from_label(&lifecycle).ok_or_else(|| {
                            StateStoreError::CorruptState {
                                detail: format!(
                                    "managed environment '{project_id}' has unknown lifecycle '{lifecycle}'"
                                ),
                            }
                        })?;

                    Ok(ManagedEnvironmentRecord::new(
                        ManagedEnvironmentRecordOptions {
                            project_id,
                            revision,
                            values,
                            lifecycle,
                        },
                    ))
                },
            )
            .collect()
    }
}

struct PersistedResourceOwnership {
    installation_id: String,
    kind: String,
    compatibility_fingerprint: String,
    project_id: Option<String>,
    schema_version: i64,
    retention: String,
    lifecycle: String,
}

impl PersistedResourceOwnership {
    fn matches(&self, resource: &ResourceRecord) -> bool {
        self.installation_id == resource.installation_id()
            && self.kind == resource.kind()
            && self.compatibility_fingerprint == resource.compatibility_fingerprint()
            && self.project_id.as_deref() == resource.project_id()
            && self.schema_version == i64::from(resource.schema_version())
            && self.retention == resource.retention().label()
    }

    fn is_active(&self) -> bool {
        self.lifecycle == ResourceLifecycle::Active.label()
    }
}

fn load_resource_ownership(
    connection: &Connection,
    resource_id: &str,
) -> Result<Option<PersistedResourceOwnership>, StateStoreError> {
    connection
        .query_row(
            "SELECT installation_id, kind, compatibility_fingerprint, project_id,
                    resource_schema_version, retention, lifecycle
             FROM resources WHERE resource_id = ?1",
            [resource_id],
            |row| {
                Ok(PersistedResourceOwnership {
                    installation_id: row.get(0)?,
                    kind: row.get(1)?,
                    compatibility_fingerprint: row.get(2)?,
                    project_id: row.get(3)?,
                    schema_version: row.get(4)?,
                    retention: row.get(5)?,
                    lifecycle: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn load_credential(
    connection: &Connection,
    credential_id: &str,
) -> Result<Option<CredentialRecord>, StateStoreError> {
    let persisted = connection
        .query_row(
            "SELECT credential_id, project_id, service_id, username, secret, lifecycle\n\
             FROM credentials WHERE credential_id = ?1",
            [credential_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;

    persisted.map(credential_from_persisted).transpose()
}

fn credential_from_persisted(
    persisted: (String, Option<String>, String, String, String, String),
) -> Result<CredentialRecord, StateStoreError> {
    let (credential_id, project_id, service_id, username, secret, lifecycle) = persisted;
    let lifecycle = CredentialLifecycle::from_label(&lifecycle).ok_or_else(|| {
        StateStoreError::CorruptState {
            detail: format!("credential '{credential_id}' has unknown lifecycle '{lifecycle}'"),
        }
    })?;

    Ok(CredentialRecord::new(CredentialRecordOptions {
        credential_id,
        project_id,
        service_id,
        username,
        secret,
        lifecycle,
    }))
}

fn load_installation(
    connection: &Connection,
) -> Result<Option<InstallationRecord>, StateStoreError> {
    let persisted = connection
        .query_row(
            "SELECT installation_id, engine_provider, engine_endpoint\n\
             FROM installation WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((installation_id, engine_provider, engine_endpoint)) = persisted else {
        return Ok(None);
    };
    let engine_provider = EngineProvider::from_label(&engine_provider).ok_or_else(|| {
        StateStoreError::CorruptState {
            detail: format!("installation has unknown Engine provider '{engine_provider}'"),
        }
    })?;

    Ok(Some(InstallationRecord::new(
        installation_id,
        engine_provider,
        engine_endpoint,
    )))
}

fn exact_path(path: &Path) -> Result<&str, StateStoreError> {
    path.to_str().ok_or_else(|| StateStoreError::NonUtf8Path {
        path: path.to_path_buf(),
    })
}
