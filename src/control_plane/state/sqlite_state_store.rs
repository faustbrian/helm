use super::credential_persistence::{
    credential_from_persisted, load_credential, persist_credential_if_absent,
};
use super::logical_resource_persistence::{
    load_logical_resource_ownership, persist_logical_resources,
};
use super::persist_managed_environment::persist_managed_environment;
use super::persist_migration_record::persist_migration_record;
use super::persisted_migration::PersistedMigration;
use super::{
    CredentialLifecycle, CredentialRecord, DaemonEventRecord, DaemonOperationRecord,
    DaemonOperationRecordOptions, DaemonOperationStatus, DaemonOperationTransitionOptions,
    EngineProvider, EnvironmentLifecycle, InstallationLifecycle, InstallationRecord,
    LogicalResourceRecord, LogicalResourceRecordOptions, ManagedEnvironmentRecord,
    ManagedEnvironmentRecordOptions, MigrationPhase, MigrationRecord, ProjectAdoptionPlan,
    ProjectRecord, RecoveryPointRecord, RecoveryPointRecordOptions, ResourceLifecycle,
    ResourceRecord, ResourceRecordOptions, ResourceRetention, StateStore, StateStoreError,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const CURRENT_SCHEMA_VERSION: u32 = 14;

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

        if found < 7 {
            transaction.execute_batch(
                "CREATE TABLE logical_resources (
                     logical_resource_id TEXT PRIMARY KEY NOT NULL,
                     shared_resource_id TEXT NOT NULL CHECK(length(shared_resource_id) > 0),
                     project_id TEXT NOT NULL CHECK(length(project_id) > 0),
                     service_id TEXT NOT NULL CHECK(length(service_id) > 0),
                     kind TEXT NOT NULL CHECK(length(kind) > 0),
                     compatibility_fingerprint TEXT NOT NULL
                         CHECK(length(compatibility_fingerprint) > 0),
                     desired_revision TEXT NOT NULL CHECK(length(desired_revision) > 0),
                     lifecycle TEXT NOT NULL
                         CHECK(lifecycle IN ('active', 'orphaned', 'retained')),
                     orphaned_at_unix_seconds INTEGER
                 ) STRICT;
                 CREATE INDEX logical_resources_project_idx
                     ON logical_resources(project_id);
                 CREATE INDEX logical_resources_shared_idx
                     ON logical_resources(shared_resource_id, lifecycle);",
            )?;
        }

        if found < 8 {
            transaction.execute_batch(
                "CREATE TABLE migrations (
                     migration_id TEXT PRIMARY KEY NOT NULL CHECK(length(migration_id) > 0),
                     project_id TEXT NOT NULL CHECK(length(project_id) > 0),
                     source_revision TEXT NOT NULL CHECK(length(source_revision) > 0),
                     target_revision TEXT NOT NULL CHECK(length(target_revision) > 0),
                     source_compatibility_fingerprint TEXT NOT NULL
                         CHECK(length(source_compatibility_fingerprint) > 0),
                     target_compatibility_fingerprint TEXT NOT NULL
                         CHECK(length(target_compatibility_fingerprint) > 0),
                     phase TEXT NOT NULL CHECK(phase IN (
                         'inventoried', 'backup_verified', 'target_provisioned',
                         'data_restored', 'target_verified', 'cutover',
                         'confirmed', 'rolled_back'
                     )),
                     backup_artifact_sha256 TEXT,
                     backup_artifact_size_bytes INTEGER
                         CHECK(backup_artifact_size_bytes >= 0),
                     target_resource_id TEXT,
                     rollback_reference TEXT,
                     updated_at_unix_seconds INTEGER NOT NULL
                         CHECK(updated_at_unix_seconds >= 0)
                 ) STRICT;
                 CREATE INDEX migrations_project_idx ON migrations(project_id);",
            )?;
        }

        if found < 9 {
            transaction
                .execute_batch("ALTER TABLE migrations ADD COLUMN backup_reference TEXT;")?;
        }

        if found < 10 {
            transaction.execute_batch(
                "ALTER TABLE resources ADD COLUMN scope_id TEXT
                     CHECK(scope_id IS NULL OR length(scope_id) > 0);
                 CREATE INDEX resources_scope_idx
                     ON resources(installation_id, kind, project_id, scope_id);",
            )?;
        }

        if found < 11 {
            transaction.execute_batch(
                "CREATE TABLE daemon_events (
                     sequence INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                     operation_id TEXT NOT NULL CHECK(length(operation_id) > 0),
                     kind_json TEXT NOT NULL CHECK(length(kind_json) > 0)
                 ) STRICT;",
            )?;
        }

        if found < 12 {
            transaction.execute_batch(
                "CREATE TABLE daemon_operations (
                     operation_id TEXT PRIMARY KEY NOT NULL CHECK(length(operation_id) > 0),
                     kind TEXT NOT NULL CHECK(length(kind) > 0),
                     payload_json TEXT NOT NULL CHECK(length(payload_json) > 0),
                     status TEXT NOT NULL CHECK(status IN (
                         'queued', 'running', 'completed', 'failed', 'cancelled'
                     )),
                     created_at_unix_seconds INTEGER NOT NULL
                         CHECK(created_at_unix_seconds >= 0),
                     updated_at_unix_seconds INTEGER NOT NULL
                         CHECK(updated_at_unix_seconds >= created_at_unix_seconds)
                 ) STRICT;
                 CREATE INDEX daemon_operations_active_idx
                     ON daemon_operations(status, created_at_unix_seconds);",
            )?;
        }

        if found < 13 {
            transaction.execute_batch(
                "CREATE TABLE recovery_points (
                     recovery_point_id TEXT PRIMARY KEY NOT NULL
                         CHECK(length(recovery_point_id) > 0),
                     project_id TEXT NOT NULL CHECK(length(project_id) > 0),
                     service_id TEXT NOT NULL CHECK(length(service_id) > 0),
                     logical_resource_id TEXT NOT NULL
                         CHECK(length(logical_resource_id) > 0),
                     resource_kind TEXT NOT NULL CHECK(length(resource_kind) > 0),
                     compatibility_fingerprint TEXT NOT NULL
                         CHECK(length(compatibility_fingerprint) > 0),
                     reference TEXT NOT NULL CHECK(length(reference) > 0),
                     artifact_sha256 TEXT NOT NULL CHECK(length(artifact_sha256) = 64),
                     artifact_size_bytes INTEGER NOT NULL
                         CHECK(artifact_size_bytes > 0),
                     created_at_unix_seconds INTEGER NOT NULL
                         CHECK(created_at_unix_seconds >= 0),
                     verified_at_unix_seconds INTEGER NOT NULL
                         CHECK(verified_at_unix_seconds >= created_at_unix_seconds)
                 ) STRICT;
                 CREATE INDEX recovery_points_project_idx
                     ON recovery_points(project_id, created_at_unix_seconds DESC);",
            )?;
        }

        if found < 14 {
            let installation_exists = transaction.query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM sqlite_master
                     WHERE type = 'table' AND name = 'installation'
                 )",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            if installation_exists {
                transaction.execute_batch(
                    "ALTER TABLE installation ADD COLUMN lifecycle TEXT NOT NULL
                         DEFAULT 'active' CHECK(lifecycle IN ('active', 'deleting'));",
                )?;
            } else {
                transaction.execute_batch(
                    "CREATE TABLE installation (
                         singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
                         installation_id TEXT NOT NULL CHECK(length(installation_id) > 0),
                         engine_provider TEXT NOT NULL CHECK(engine_provider IN ('docker')),
                         engine_endpoint TEXT NOT NULL CHECK(length(engine_endpoint) > 0),
                         lifecycle TEXT NOT NULL DEFAULT 'active'
                             CHECK(lifecycle IN ('active', 'deleting'))
                     ) STRICT;",
                )?;
            }
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

    fn installation_lifecycle(&self) -> Result<Option<InstallationLifecycle>, StateStoreError> {
        load_installation_lifecycle(&self.connection)
    }

    fn begin_installation_deletion(
        &mut self,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError> {
        if orphaned_at_unix_seconds < 0 {
            return Err(StateStoreError::CorruptState {
                detail: "installation deletion time must not be negative".to_owned(),
            });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE installation SET lifecycle = 'deleting' WHERE singleton = 1",
            [],
        )?;
        if updated != 1 {
            return Err(StateStoreError::CorruptState {
                detail: "installation deletion requires initialized state".to_owned(),
            });
        }
        let projects = {
            let mut statement =
                transaction.prepare("SELECT project_name FROM projects ORDER BY canonical_path")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for project_id in projects {
            orphan_project_state(&transaction, &project_id, orphaned_at_unix_seconds)?;
        }
        transaction.execute("DELETE FROM route_claims", [])?;
        transaction.execute("DELETE FROM projects", [])?;
        transaction.execute("DELETE FROM watched_roots", [])?;
        transaction.commit()?;

        Ok(())
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
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_reconciliation_active(&transaction)?;
        replace_project_batch(&transaction, &exact_projects)?;
        transaction.commit()?;

        Ok(())
    }

    fn reconcile_project_registry(
        &mut self,
        projects: &[ProjectRecord],
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError> {
        let exact_projects = projects
            .iter()
            .map(|project| exact_path(project.canonical_path()).map(|path| (project, path)))
            .collect::<Result<Vec<_>, _>>()?;
        let desired_paths = exact_projects
            .iter()
            .map(|(_, path)| *path)
            .collect::<BTreeSet<_>>();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_reconciliation_active(&transaction)?;

        replace_project_batch(&transaction, &exact_projects)?;
        let existing_projects = {
            let mut statement = transaction.prepare(
                "SELECT canonical_path, project_name FROM projects ORDER BY canonical_path",
            )?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        for (canonical_path, project_id) in existing_projects {
            if desired_paths.contains(canonical_path.as_str()) {
                continue;
            }
            orphan_project_state(&transaction, &project_id, orphaned_at_unix_seconds)?;
            transaction.execute(
                "DELETE FROM projects WHERE canonical_path = ?1",
                [canonical_path],
            )?;
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
            orphan_project_state(&transaction, &project_id, orphaned_at_unix_seconds)?;
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
                     resource_id, scope_id, installation_id, kind, compatibility_fingerprint,\n\
                     project_id, resource_schema_version, desired_revision, retention,\n\
                     lifecycle, orphaned_at_unix_seconds\n\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)\n\
                 ON CONFLICT(resource_id) DO UPDATE SET\n\
                     desired_revision = excluded.desired_revision,\n\
                     lifecycle = excluded.lifecycle,\n\
                     orphaned_at_unix_seconds = excluded.orphaned_at_unix_seconds",
                params![
                    resource.resource_id(),
                    resource.scope_id(),
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

    fn reconcile_resources(
        &mut self,
        resources: &[ResourceRecord],
        replaced_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError> {
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
        }

        for resource in resources {
            transaction.execute(
                "UPDATE resources
                 SET lifecycle = ?1, orphaned_at_unix_seconds = ?2
                 WHERE installation_id = ?3
                   AND kind = ?4
                   AND (
                       (?5 IS NOT NULL AND scope_id = ?5)
                       OR (
                           ?5 IS NULL
                           AND scope_id IS NULL
                           AND compatibility_fingerprint = ?6
                       )
                   )
                   AND project_id IS ?7
                   AND resource_schema_version = ?8
                   AND retention = ?9
                   AND lifecycle = ?10
                   AND resource_id <> ?11",
                params![
                    ResourceLifecycle::Retained.label(),
                    replaced_at_unix_seconds,
                    resource.installation_id(),
                    resource.kind(),
                    resource.scope_id(),
                    resource.compatibility_fingerprint(),
                    resource.project_id(),
                    resource.schema_version(),
                    resource.retention().label(),
                    ResourceLifecycle::Active.label(),
                    resource.resource_id(),
                ],
            )?;
            transaction.execute(
                "INSERT INTO resources (
                     resource_id, scope_id, installation_id, kind, compatibility_fingerprint,
                     project_id, resource_schema_version, desired_revision, retention,
                     lifecycle, orphaned_at_unix_seconds
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(resource_id) DO UPDATE SET
                     desired_revision = excluded.desired_revision,
                     lifecycle = excluded.lifecycle,
                     orphaned_at_unix_seconds = excluded.orphaned_at_unix_seconds",
                params![
                    resource.resource_id(),
                    resource.scope_id(),
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
        for resource in adoption.logical_resources() {
            let existing =
                load_logical_resource_ownership(&transaction, resource.logical_resource_id())?
                    .ok_or_else(|| StateStoreError::ProjectAdoptionStateMismatch {
                        project_id: adoption.project_id().to_owned(),
                        detail: format!(
                            "logical resource '{}' is missing",
                            resource.logical_resource_id()
                        ),
                    })?;
            if !existing.matches(resource) {
                return Err(StateStoreError::LogicalResourceOwnershipConflict {
                    logical_resource_id: resource.logical_resource_id().to_owned(),
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
        for resource in adoption.logical_resources() {
            transaction.execute(
                "UPDATE logical_resources
                 SET desired_revision = ?1, lifecycle = 'active',
                     orphaned_at_unix_seconds = NULL
                 WHERE logical_resource_id = ?2",
                params![resource.desired_revision(), resource.logical_resource_id()],
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
                    lifecycle, orphaned_at_unix_seconds, scope_id\n\
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
                row.get::<_, Option<String>>(10)?,
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
                    scope_id,
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

                    let resource = ResourceRecord::new(ResourceRecordOptions {
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
                    });

                    Ok(match scope_id {
                        Some(scope_id) => resource.with_scope_id(scope_id),
                        None => resource,
                    })
                },
            )
            .collect()
    }

    fn retire_resources(&mut self, resources: &[ResourceRecord]) -> Result<(), StateStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut present = Vec::with_capacity(resources.len());

        for resource in resources {
            let existing = load_resource_ownership(&transaction, resource.resource_id())?;
            let Some(existing) = existing else {
                present.push(false);
                continue;
            };
            if existing.is_active() {
                return Err(StateStoreError::InvalidResourceRetirement {
                    resource_id: resource.resource_id().to_owned(),
                    detail: "active ownership must be orphaned before deletion".to_owned(),
                });
            }
            if !existing.matches_snapshot(resource) {
                return Err(StateStoreError::InvalidResourceRetirement {
                    resource_id: resource.resource_id().to_owned(),
                    detail: "the requested snapshot differs from durable ownership".to_owned(),
                });
            }
            present.push(true);
        }

        for (resource, present) in resources.iter().zip(present) {
            if present {
                transaction.execute(
                    "DELETE FROM resources WHERE resource_id = ?1",
                    [resource.resource_id()],
                )?;
            }
        }
        transaction.commit()?;

        Ok(())
    }

    fn upsert_logical_resources(
        &mut self,
        resources: &[LogicalResourceRecord],
    ) -> Result<(), StateStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        persist_logical_resources(&transaction, resources)?;
        transaction.commit()?;

        Ok(())
    }

    fn logical_resources(&self) -> Result<Vec<LogicalResourceRecord>, StateStoreError> {
        let mut statement = self.connection.prepare(
            "SELECT logical_resource_id, shared_resource_id, project_id, service_id,
                    kind, compatibility_fingerprint, desired_revision, lifecycle,
                    orphaned_at_unix_seconds
             FROM logical_resources ORDER BY logical_resource_id",
        )?;
        let persisted = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        persisted
            .into_iter()
            .map(
                |(
                    logical_resource_id,
                    shared_resource_id,
                    project_id,
                    service_id,
                    kind,
                    compatibility_fingerprint,
                    desired_revision,
                    lifecycle,
                    orphaned_at_unix_seconds,
                )| {
                    let lifecycle =
                        ResourceLifecycle::from_label(&lifecycle).ok_or_else(|| {
                            StateStoreError::CorruptState {
                                detail: format!(
                                    "logical resource '{logical_resource_id}' has unknown lifecycle '{lifecycle}'"
                                ),
                            }
                        })?;

                    Ok(LogicalResourceRecord::new(LogicalResourceRecordOptions {
                        logical_resource_id,
                        shared_resource_id,
                        project_id,
                        service_id,
                        kind,
                        compatibility_fingerprint,
                        desired_revision,
                        lifecycle,
                        orphaned_at_unix_seconds,
                    }))
                },
            )
            .collect()
    }

    fn active_logical_reference_count(
        &self,
        shared_resource_id: &str,
    ) -> Result<u64, StateStoreError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM logical_resources
             WHERE shared_resource_id = ?1 AND lifecycle = 'active'",
            [shared_resource_id],
            |row| row.get::<_, i64>(0),
        )?;

        u64::try_from(count).map_err(|_| StateStoreError::CorruptState {
            detail: format!(
                "shared resource '{shared_resource_id}' has invalid active reference count {count}"
            ),
        })
    }

    fn retire_logical_resource(
        &mut self,
        resource: &LogicalResourceRecord,
        credential: &CredentialRecord,
    ) -> Result<(), StateStoreError> {
        let invalid_input = resource.lifecycle() == ResourceLifecycle::Active
            || resource.orphaned_at_unix_seconds().is_none()
            || credential.lifecycle() != CredentialLifecycle::Disabled
            || credential.project_id() != Some(resource.project_id())
            || credential.service_id() != resource.service_id();
        if invalid_input {
            return Err(invalid_logical_retirement(
                resource,
                "only an exact orphaned tenant and disabled credential can be retired",
            ));
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let project_registered = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE project_name = ?1)",
            [resource.project_id()],
            |row| row.get::<_, bool>(0),
        )?;
        if project_registered {
            return Err(invalid_logical_retirement(
                resource,
                "the owning project is registered",
            ));
        }
        let existing_resource =
            load_logical_resource_ownership(&transaction, resource.logical_resource_id())?;
        let existing_credential = load_credential(&transaction, credential.credential_id())?;
        if existing_resource.is_none() && existing_credential.is_none() {
            transaction.commit()?;

            return Ok(());
        }
        let exact = existing_resource
            .as_ref()
            .is_some_and(|existing| existing.matches_snapshot(resource))
            && existing_credential.as_ref() == Some(credential);
        if !exact {
            return Err(invalid_logical_retirement(
                resource,
                "the requested snapshot differs from durable ownership",
            ));
        }

        transaction.execute(
            "DELETE FROM logical_resources WHERE logical_resource_id = ?1",
            [resource.logical_resource_id()],
        )?;
        transaction.execute(
            "DELETE FROM credentials WHERE credential_id = ?1",
            [credential.credential_id()],
        )?;
        let remaining_project_state = transaction.query_row(
            "SELECT
                 (SELECT COUNT(*) FROM logical_resources WHERE project_id = ?1) +
                 (SELECT COUNT(*) FROM credentials WHERE project_id = ?1)",
            [resource.project_id()],
            |row| row.get::<_, i64>(0),
        )?;
        if remaining_project_state == 0 {
            transaction.execute(
                "DELETE FROM managed_environments WHERE project_id = ?1",
                [resource.project_id()],
            )?;
        }
        transaction.commit()?;

        Ok(())
    }

    fn insert_credential_if_absent(
        &mut self,
        credential: &CredentialRecord,
    ) -> Result<CredentialRecord, StateStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let persisted = persist_credential_if_absent(&transaction, credential)?;
        transaction.commit()?;

        Ok(persisted)
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
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        persist_managed_environment(&transaction, environment)?;
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

    fn record_logical_environment(
        &mut self,
        resources: &[LogicalResourceRecord],
        environment: &ManagedEnvironmentRecord,
    ) -> Result<(), StateStoreError> {
        if resources.is_empty() {
            return Err(StateStoreError::InvalidLogicalEnvironment {
                detail: "at least one logical resource is required".to_owned(),
            });
        }
        if let Some(resource) = resources
            .iter()
            .find(|resource| resource.project_id() != environment.project_id())
        {
            return Err(StateStoreError::InvalidLogicalEnvironment {
                detail: format!(
                    "resource '{}' belongs to project '{}', not environment project '{}'",
                    resource.logical_resource_id(),
                    resource.project_id(),
                    environment.project_id()
                ),
            });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        persist_logical_resources(&transaction, resources)?;
        persist_managed_environment(&transaction, environment)?;
        transaction.commit()?;

        Ok(())
    }

    fn reconcile_logical_environment(
        &mut self,
        resources: &[LogicalResourceRecord],
        environment: &ManagedEnvironmentRecord,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError> {
        if orphaned_at_unix_seconds < 0 {
            return Err(StateStoreError::InvalidLogicalEnvironment {
                detail: "logical orphan time must not be negative".to_owned(),
            });
        }
        if let Some(resource) = resources
            .iter()
            .find(|resource| resource.project_id() != environment.project_id())
        {
            return Err(StateStoreError::InvalidLogicalEnvironment {
                detail: format!(
                    "resource '{}' belongs to project '{}', not environment project '{}'",
                    resource.logical_resource_id(),
                    resource.project_id(),
                    environment.project_id()
                ),
            });
        }
        let desired_resources = resources
            .iter()
            .map(LogicalResourceRecord::logical_resource_id)
            .collect::<BTreeSet<_>>();
        let desired_services = resources
            .iter()
            .map(LogicalResourceRecord::service_id)
            .collect::<BTreeSet<_>>();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active_resources = {
            let mut statement = transaction.prepare(
                "SELECT logical_resource_id FROM logical_resources
                 WHERE project_id = ?1 AND lifecycle = 'active'",
            )?;
            statement
                .query_map([environment.project_id()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for logical_resource_id in active_resources {
            if !desired_resources.contains(logical_resource_id.as_str()) {
                transaction.execute(
                    "UPDATE logical_resources
                     SET lifecycle = 'orphaned', orphaned_at_unix_seconds = ?1
                     WHERE logical_resource_id = ?2",
                    params![orphaned_at_unix_seconds, logical_resource_id],
                )?;
            }
        }
        let active_credentials = {
            let mut statement = transaction.prepare(
                "SELECT credential_id, service_id FROM credentials
                 WHERE project_id = ?1 AND lifecycle = 'active'",
            )?;
            statement
                .query_map([environment.project_id()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        for (credential_id, service_id) in active_credentials {
            if !desired_services.contains(service_id.as_str()) {
                transaction.execute(
                    "UPDATE credentials SET lifecycle = 'disabled' WHERE credential_id = ?1",
                    [credential_id],
                )?;
            }
        }
        persist_logical_resources(&transaction, resources)?;
        persist_managed_environment(&transaction, environment)?;
        transaction.commit()?;

        Ok(())
    }

    fn record_migration(&mut self, migration: &MigrationRecord) -> Result<(), StateStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        persist_migration_record(&transaction, migration)?;
        transaction.commit()?;

        Ok(())
    }

    fn record_migration_target(
        &mut self,
        target: &LogicalResourceRecord,
        credential: &CredentialRecord,
        migration: &MigrationRecord,
    ) -> Result<(), StateStoreError> {
        if migration.phase() != MigrationPhase::TargetProvisioned
            || target.lifecycle() != ResourceLifecycle::Active
            || credential.lifecycle() != CredentialLifecycle::Active
            || target.project_id() != migration.project_id()
            || credential.project_id() != Some(migration.project_id())
            || credential.service_id() != target.service_id()
            || target.compatibility_fingerprint() != migration.target_compatibility_fingerprint()
        {
            return Err(StateStoreError::InvalidMigrationTarget {
                detail: "active logical ownership, credential, and target checkpoint must match"
                    .to_owned(),
            });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let registered_project = transaction
            .query_row(
                "SELECT 1 FROM projects WHERE project_name = ?1",
                [migration.project_id()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if registered_project != Some(1) {
            return Err(StateStoreError::InvalidMigrationTarget {
                detail: "the exact migration project is not registered".to_owned(),
            });
        }
        persist_logical_resources(&transaction, std::slice::from_ref(target))?;
        let stable_credential = persist_credential_if_absent(&transaction, credential)?;
        if stable_credential != *credential {
            return Err(StateStoreError::MigrationTargetCredentialConflict {
                credential_id: credential.credential_id().to_owned(),
            });
        }
        persist_migration_record(&transaction, migration)?;
        transaction.commit()?;

        Ok(())
    }

    fn record_migration_cutover(
        &mut self,
        project: &ProjectRecord,
        environment: &ManagedEnvironmentRecord,
        migration: &MigrationRecord,
    ) -> Result<(), StateStoreError> {
        if migration.phase() != MigrationPhase::Cutover
            || migration.project_id() != project.project_name()
            || migration.project_id() != environment.project_id()
            || environment.lifecycle() != EnvironmentLifecycle::Active
        {
            return Err(StateStoreError::InvalidMigrationCutover {
                detail: "project, active environment, and cutover checkpoint must match".to_owned(),
            });
        }
        let canonical_path = exact_path(project.canonical_path())?;
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
        if registered_project.as_deref() != Some(migration.project_id()) {
            return Err(StateStoreError::InvalidMigrationCutover {
                detail: "the exact migration project path is not registered".to_owned(),
            });
        }

        replace_project_batch(&transaction, &[(project, canonical_path)])?;
        persist_managed_environment(&transaction, environment)?;
        persist_migration_record(&transaction, migration)?;
        transaction.commit()?;

        Ok(())
    }

    fn record_migration_rollback(
        &mut self,
        project: &ProjectRecord,
        environment: &ManagedEnvironmentRecord,
        retained_targets: &[LogicalResourceRecord],
        migration: &MigrationRecord,
    ) -> Result<(), StateStoreError> {
        if migration.phase() != MigrationPhase::RolledBack
            || migration.project_id() != project.project_name()
            || migration.project_id() != environment.project_id()
            || environment.lifecycle() != EnvironmentLifecycle::Active
            || retained_targets.iter().any(|target| {
                target.project_id() != migration.project_id()
                    || target.lifecycle() != ResourceLifecycle::Retained
            })
        {
            return Err(StateStoreError::InvalidMigrationRollback {
                detail: "project, active environment, retained targets, and rollback checkpoint must match"
                    .to_owned(),
            });
        }
        let canonical_path = exact_path(project.canonical_path())?;
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
        if registered_project.as_deref() != Some(migration.project_id()) {
            return Err(StateStoreError::InvalidMigrationRollback {
                detail: "the exact migration project path is not registered".to_owned(),
            });
        }

        replace_project_batch(&transaction, &[(project, canonical_path)])?;
        persist_managed_environment(&transaction, environment)?;
        persist_logical_resources(&transaction, retained_targets)?;
        persist_migration_record(&transaction, migration)?;
        transaction.commit()?;

        Ok(())
    }

    fn migrations(&self) -> Result<Vec<MigrationRecord>, StateStoreError> {
        let mut statement = self.connection.prepare(
            "SELECT migration_id, project_id, source_revision, target_revision,
                    source_compatibility_fingerprint,
                    target_compatibility_fingerprint, phase,
                    backup_reference, backup_artifact_sha256,
                    backup_artifact_size_bytes,
                    target_resource_id, rollback_reference,
                    updated_at_unix_seconds
             FROM migrations ORDER BY migration_id",
        )?;
        let persisted = statement
            .query_map([], |row| {
                Ok(PersistedMigration {
                    migration_id: row.get(0)?,
                    project_id: row.get(1)?,
                    source_revision: row.get(2)?,
                    target_revision: row.get(3)?,
                    source_compatibility_fingerprint: row.get(4)?,
                    target_compatibility_fingerprint: row.get(5)?,
                    phase: row.get(6)?,
                    backup_reference: row.get(7)?,
                    backup_artifact_sha256: row.get(8)?,
                    backup_artifact_size_bytes: row.get(9)?,
                    target_resource_id: row.get(10)?,
                    rollback_reference: row.get(11)?,
                    updated_at_unix_seconds: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        persisted
            .into_iter()
            .map(PersistedMigration::into_record)
            .collect()
    }

    fn record_recovery_point(
        &mut self,
        recovery_point: &RecoveryPointRecord,
    ) -> Result<(), StateStoreError> {
        let size = i64::try_from(recovery_point.artifact_size_bytes()).map_err(|_| {
            StateStoreError::CorruptState {
                detail: "recovery point artifact size exceeds SQLite limits".to_owned(),
            }
        })?;
        let existing = self
            .connection
            .query_row(
                "SELECT project_id, service_id, logical_resource_id, resource_kind,
                        compatibility_fingerprint, reference, artifact_sha256,
                        artifact_size_bytes, created_at_unix_seconds,
                        verified_at_unix_seconds
                 FROM recovery_points WHERE recovery_point_id = ?1",
                [recovery_point.recovery_point_id()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                },
            )
            .optional()?;
        if let Some(existing) = existing {
            let matches = existing.0 == recovery_point.project_id()
                && existing.1 == recovery_point.service_id()
                && existing.2 == recovery_point.logical_resource_id()
                && existing.3 == recovery_point.resource_kind()
                && existing.4 == recovery_point.compatibility_fingerprint()
                && existing.5 == recovery_point.reference()
                && existing.6 == recovery_point.artifact_sha256()
                && existing.7 == size
                && existing.8 == recovery_point.created_at_unix_seconds()
                && existing.9 == recovery_point.verified_at_unix_seconds();
            if matches {
                return Ok(());
            }

            return Err(StateStoreError::RecoveryPointEvidenceConflict {
                recovery_point_id: recovery_point.recovery_point_id().to_owned(),
            });
        }
        self.connection.execute(
            "INSERT INTO recovery_points (
                 recovery_point_id, project_id, service_id, logical_resource_id,
                 resource_kind, compatibility_fingerprint, reference,
                 artifact_sha256, artifact_size_bytes, created_at_unix_seconds,
                 verified_at_unix_seconds
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                recovery_point.recovery_point_id(),
                recovery_point.project_id(),
                recovery_point.service_id(),
                recovery_point.logical_resource_id(),
                recovery_point.resource_kind(),
                recovery_point.compatibility_fingerprint(),
                recovery_point.reference(),
                recovery_point.artifact_sha256(),
                size,
                recovery_point.created_at_unix_seconds(),
                recovery_point.verified_at_unix_seconds(),
            ],
        )?;

        Ok(())
    }

    fn recovery_points(
        &self,
        project_id: &str,
    ) -> Result<Vec<RecoveryPointRecord>, StateStoreError> {
        if project_id.is_empty() {
            return Err(StateStoreError::CorruptState {
                detail: "recovery point project ID must not be empty".to_owned(),
            });
        }
        let mut statement = self.connection.prepare(
            "SELECT recovery_point_id, project_id, service_id,
                    logical_resource_id, resource_kind, compatibility_fingerprint,
                    reference, artifact_sha256, artifact_size_bytes,
                    created_at_unix_seconds, verified_at_unix_seconds
             FROM recovery_points WHERE project_id = ?1
             ORDER BY created_at_unix_seconds DESC, recovery_point_id DESC",
        )?;
        let persisted = statement
            .query_map([project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        persisted
            .into_iter()
            .map(
                |(
                    recovery_point_id,
                    project_id,
                    service_id,
                    logical_resource_id,
                    resource_kind,
                    compatibility_fingerprint,
                    reference,
                    artifact_sha256,
                    artifact_size_bytes,
                    created_at_unix_seconds,
                    verified_at_unix_seconds,
                )| {
                    let artifact_size_bytes = u64::try_from(artifact_size_bytes).map_err(|_| {
                        StateStoreError::CorruptState {
                            detail: "recovery point contains an invalid artifact size".to_owned(),
                        }
                    })?;
                    RecoveryPointRecord::new(RecoveryPointRecordOptions {
                        recovery_point_id,
                        project_id,
                        service_id,
                        logical_resource_id,
                        resource_kind,
                        compatibility_fingerprint,
                        reference,
                        artifact_sha256,
                        artifact_size_bytes,
                        created_at_unix_seconds,
                        verified_at_unix_seconds,
                    })
                    .map_err(|detail| StateStoreError::CorruptState { detail })
                },
            )
            .collect()
    }

    fn append_daemon_event(
        &mut self,
        operation_id: &str,
        kind_json: &str,
        retention_limit: usize,
    ) -> Result<DaemonEventRecord, StateStoreError> {
        if operation_id.is_empty() || kind_json.is_empty() || retention_limit == 0 {
            return Err(StateStoreError::InvalidDaemonEvent {
                detail: "operation ID, event payload, and retention limit must be non-empty"
                    .to_owned(),
            });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let event = append_daemon_event_in_transaction(
            &transaction,
            operation_id,
            kind_json,
            retention_limit,
        )?;
        transaction.commit()?;

        Ok(event)
    }

    fn daemon_events(&self) -> Result<Vec<DaemonEventRecord>, StateStoreError> {
        let mut statement = self.connection.prepare(
            "SELECT sequence, operation_id, kind_json
             FROM daemon_events ORDER BY sequence",
        )?;
        let persisted = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        persisted
            .into_iter()
            .map(|(sequence, operation_id, kind_json)| {
                let sequence =
                    u64::try_from(sequence).map_err(|_| StateStoreError::CorruptState {
                        detail: "daemon event sequence is outside the supported range".to_owned(),
                    })?;
                Ok(DaemonEventRecord::new(sequence, operation_id, kind_json))
            })
            .collect()
    }

    fn enqueue_daemon_operation(
        &mut self,
        operation: &DaemonOperationRecord,
        accepted_kind_json: &str,
        event_retention_limit: usize,
    ) -> Result<DaemonEventRecord, StateStoreError> {
        validate_new_daemon_operation(operation, accepted_kind_json)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO daemon_operations (
                 operation_id, kind, payload_json, status,
                 created_at_unix_seconds, updated_at_unix_seconds
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                operation.operation_id(),
                operation.kind(),
                operation.payload_json(),
                operation.status().label(),
                operation.created_at_unix_seconds(),
                operation.updated_at_unix_seconds(),
            ],
        )?;
        let event = append_daemon_event_in_transaction(
            &transaction,
            operation.operation_id(),
            accepted_kind_json,
            event_retention_limit,
        )?;
        transaction.commit()?;

        Ok(event)
    }

    fn transition_daemon_operation(
        &mut self,
        options: DaemonOperationTransitionOptions<'_>,
    ) -> Result<Option<DaemonEventRecord>, StateStoreError> {
        let DaemonOperationTransitionOptions {
            operation_id,
            expected,
            next,
            updated_at_unix_seconds,
            event_kind_json,
            event_retention_limit,
        } = options;
        validate_daemon_operation_transition(
            operation_id,
            expected,
            next,
            updated_at_unix_seconds,
            event_kind_json,
            event_retention_limit,
        )?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE daemon_operations
             SET status = ?1, updated_at_unix_seconds = ?2
             WHERE operation_id = ?3 AND status = ?4
               AND updated_at_unix_seconds <= ?2",
            params![
                next.label(),
                updated_at_unix_seconds,
                operation_id,
                expected.label(),
            ],
        )?;
        if changed != 1 {
            return Err(StateStoreError::InvalidDaemonOperation {
                detail: format!(
                    "operation '{operation_id}' is missing, has moved beyond '{}', or has a later timestamp",
                    expected.label()
                ),
            });
        }
        let event = event_kind_json
            .map(|kind_json| {
                append_daemon_event_in_transaction(
                    &transaction,
                    operation_id,
                    kind_json,
                    event_retention_limit,
                )
            })
            .transpose()?;
        prune_terminal_daemon_operations(&transaction, event_retention_limit)?;
        transaction.commit()?;

        Ok(event)
    }

    fn active_daemon_operations(&self) -> Result<Vec<DaemonOperationRecord>, StateStoreError> {
        let mut statement = self.connection.prepare(
            "SELECT operation_id, kind, payload_json, status,
                    created_at_unix_seconds, updated_at_unix_seconds
             FROM daemon_operations
             WHERE status IN ('queued', 'running')
             ORDER BY created_at_unix_seconds, operation_id",
        )?;
        let persisted = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        persisted
            .into_iter()
            .map(
                |(operation_id, kind, payload_json, status, created_at, updated_at)| {
                    if operation_id.is_empty()
                        || kind.is_empty()
                        || payload_json.is_empty()
                        || created_at < 0
                        || updated_at < created_at
                    {
                        return Err(StateStoreError::CorruptState {
                            detail: "daemon operation contains invalid durable fields".to_owned(),
                        });
                    }

                    Ok(DaemonOperationRecord::new(DaemonOperationRecordOptions {
                        operation_id,
                        kind,
                        payload_json,
                        status: DaemonOperationStatus::parse(&status)?,
                        created_at_unix_seconds: created_at,
                        updated_at_unix_seconds: updated_at,
                    }))
                },
            )
            .collect()
    }
}

fn append_daemon_event_in_transaction(
    transaction: &Transaction<'_>,
    operation_id: &str,
    kind_json: &str,
    retention_limit: usize,
) -> Result<DaemonEventRecord, StateStoreError> {
    if operation_id.is_empty() || kind_json.is_empty() || retention_limit == 0 {
        return Err(StateStoreError::InvalidDaemonEvent {
            detail: "operation ID, event payload, and retention limit must be non-empty".to_owned(),
        });
    }
    let retention_limit =
        i64::try_from(retention_limit).map_err(|_| StateStoreError::InvalidDaemonEvent {
            detail: "retention limit exceeds SQLite integer range".to_owned(),
        })?;
    transaction.execute(
        "INSERT INTO daemon_events (operation_id, kind_json) VALUES (?1, ?2)",
        params![operation_id, kind_json],
    )?;
    let sequence = u64::try_from(transaction.last_insert_rowid()).map_err(|_| {
        StateStoreError::CorruptState {
            detail: "daemon event sequence is outside the supported range".to_owned(),
        }
    })?;
    transaction.execute(
        "DELETE FROM daemon_events
         WHERE sequence NOT IN (
             SELECT sequence FROM daemon_events
             ORDER BY sequence DESC LIMIT ?1
         )",
        [retention_limit],
    )?;

    Ok(DaemonEventRecord::new(
        sequence,
        operation_id.to_owned(),
        kind_json.to_owned(),
    ))
}

fn prune_terminal_daemon_operations(
    transaction: &Transaction<'_>,
    retention_limit: usize,
) -> Result<(), StateStoreError> {
    let retention_limit =
        i64::try_from(retention_limit).map_err(|_| StateStoreError::InvalidDaemonOperation {
            detail: "operation retention limit exceeds SQLite integer range".to_owned(),
        })?;
    transaction.execute(
        "DELETE FROM daemon_operations
         WHERE status IN ('completed', 'failed', 'cancelled')
           AND operation_id NOT IN (
               SELECT operation_id FROM daemon_operations
               WHERE status IN ('completed', 'failed', 'cancelled')
               ORDER BY updated_at_unix_seconds DESC, operation_id DESC LIMIT ?1
           )",
        [retention_limit],
    )?;

    Ok(())
}

fn validate_new_daemon_operation(
    operation: &DaemonOperationRecord,
    accepted_kind_json: &str,
) -> Result<(), StateStoreError> {
    if operation.operation_id().is_empty()
        || operation.kind().is_empty()
        || operation.payload_json().is_empty()
        || operation.status() != DaemonOperationStatus::Queued
        || operation.created_at_unix_seconds() < 0
        || operation.updated_at_unix_seconds() != operation.created_at_unix_seconds()
        || accepted_kind_json.is_empty()
    {
        return Err(StateStoreError::InvalidDaemonOperation {
            detail: "new operations require non-empty queued identity, payload, event, and valid timestamps"
                .to_owned(),
        });
    }

    Ok(())
}

fn validate_daemon_operation_transition(
    operation_id: &str,
    expected: DaemonOperationStatus,
    next: DaemonOperationStatus,
    updated_at_unix_seconds: i64,
    event_kind_json: Option<&str>,
    event_retention_limit: usize,
) -> Result<(), StateStoreError> {
    let allowed = matches!(
        (expected, next),
        (
            DaemonOperationStatus::Queued,
            DaemonOperationStatus::Running
        ) | (DaemonOperationStatus::Queued, DaemonOperationStatus::Failed)
            | (
                DaemonOperationStatus::Queued,
                DaemonOperationStatus::Cancelled
            )
            | (
                DaemonOperationStatus::Running,
                DaemonOperationStatus::Queued
            )
            | (
                DaemonOperationStatus::Running,
                DaemonOperationStatus::Completed
            )
            | (
                DaemonOperationStatus::Running,
                DaemonOperationStatus::Failed
            )
            | (
                DaemonOperationStatus::Running,
                DaemonOperationStatus::Cancelled
            )
    );
    if operation_id.is_empty()
        || updated_at_unix_seconds < 0
        || !allowed
        || event_retention_limit == 0
        || event_kind_json.is_some_and(str::is_empty)
    {
        return Err(StateStoreError::InvalidDaemonOperation {
            detail: format!(
                "operation '{operation_id}' cannot transition from '{}' to '{}'",
                expected.label(),
                next.label()
            ),
        });
    }

    Ok(())
}

fn replace_project_batch(
    transaction: &Transaction<'_>,
    projects: &[(&ProjectRecord, &str)],
) -> Result<(), StateStoreError> {
    let batch_paths = projects
        .iter()
        .map(|(_, path)| *path)
        .collect::<BTreeSet<_>>();
    for (project, canonical_path) in projects {
        for domain in project.route_domains() {
            let existing_path = transaction
                .query_row(
                    "SELECT canonical_path FROM route_claims WHERE domain = ?1",
                    [domain],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(existing_path) = existing_path {
                if existing_path != *canonical_path && !batch_paths.contains(existing_path.as_str())
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
    for (project, canonical_path) in projects {
        transaction.execute(
            "INSERT INTO projects (canonical_path, project_name) VALUES (?1, ?2)
             ON CONFLICT(canonical_path) DO UPDATE SET project_name = excluded.project_name",
            params![canonical_path, project.project_name()],
        )?;
        transaction.execute(
            "DELETE FROM route_claims WHERE canonical_path = ?1",
            [canonical_path],
        )?;
    }
    for (project, canonical_path) in projects {
        for domain in project.route_domains() {
            transaction.execute(
                "INSERT INTO route_claims (domain, canonical_path) VALUES (?1, ?2)",
                params![domain, canonical_path],
            )?;
        }
    }

    Ok(())
}

fn orphan_project_state(
    transaction: &Transaction<'_>,
    project_id: &str,
    orphaned_at_unix_seconds: i64,
) -> Result<(), StateStoreError> {
    transaction.execute(
        "UPDATE resources
         SET lifecycle = 'orphaned', orphaned_at_unix_seconds = ?1
         WHERE project_id = ?2 AND lifecycle = 'active'",
        params![orphaned_at_unix_seconds, project_id],
    )?;
    transaction.execute(
        "UPDATE logical_resources
         SET lifecycle = 'orphaned', orphaned_at_unix_seconds = ?1
         WHERE project_id = ?2 AND lifecycle = 'active'",
        params![orphaned_at_unix_seconds, project_id],
    )?;
    transaction.execute(
        "UPDATE credentials SET lifecycle = 'disabled'
         WHERE project_id = ?1 AND lifecycle = 'active'",
        [project_id],
    )?;
    transaction.execute(
        "UPDATE managed_environments SET lifecycle = 'disabled'
         WHERE project_id = ?1 AND lifecycle = 'active'",
        [project_id],
    )?;

    Ok(())
}

struct PersistedResourceOwnership {
    scope_id: Option<String>,
    installation_id: String,
    kind: String,
    compatibility_fingerprint: String,
    project_id: Option<String>,
    schema_version: i64,
    retention: String,
    lifecycle: String,
    desired_revision: String,
    orphaned_at_unix_seconds: Option<i64>,
}

fn invalid_logical_retirement(
    resource: &LogicalResourceRecord,
    detail: impl Into<String>,
) -> StateStoreError {
    StateStoreError::InvalidLogicalResourceRetirement {
        logical_resource_id: resource.logical_resource_id().to_owned(),
        detail: detail.into(),
    }
}

impl PersistedResourceOwnership {
    fn matches(&self, resource: &ResourceRecord) -> bool {
        self.installation_id == resource.installation_id()
            && self.scope_id.as_deref() == resource.scope_id()
            && self.kind == resource.kind()
            && self.compatibility_fingerprint == resource.compatibility_fingerprint()
            && self.project_id.as_deref() == resource.project_id()
            && self.schema_version == i64::from(resource.schema_version())
            && self.retention == resource.retention().label()
    }

    fn is_active(&self) -> bool {
        self.lifecycle == ResourceLifecycle::Active.label()
    }

    fn matches_snapshot(&self, resource: &ResourceRecord) -> bool {
        self.matches(resource)
            && self.desired_revision == resource.desired_revision()
            && self.lifecycle == resource.lifecycle().label()
            && self.orphaned_at_unix_seconds == resource.orphaned_at_unix_seconds()
    }
}

fn load_resource_ownership(
    connection: &Connection,
    resource_id: &str,
) -> Result<Option<PersistedResourceOwnership>, StateStoreError> {
    connection
        .query_row(
            "SELECT scope_id, installation_id, kind, compatibility_fingerprint, project_id,
                    resource_schema_version, retention, lifecycle, desired_revision,
                    orphaned_at_unix_seconds
             FROM resources WHERE resource_id = ?1",
            [resource_id],
            |row| {
                Ok(PersistedResourceOwnership {
                    scope_id: row.get(0)?,
                    installation_id: row.get(1)?,
                    kind: row.get(2)?,
                    compatibility_fingerprint: row.get(3)?,
                    project_id: row.get(4)?,
                    schema_version: row.get(5)?,
                    retention: row.get(6)?,
                    lifecycle: row.get(7)?,
                    desired_revision: row.get(8)?,
                    orphaned_at_unix_seconds: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
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

fn load_installation_lifecycle(
    connection: &Connection,
) -> Result<Option<InstallationLifecycle>, StateStoreError> {
    let lifecycle = connection
        .query_row(
            "SELECT lifecycle FROM installation WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    lifecycle
        .map(|label| {
            InstallationLifecycle::from_label(&label).ok_or_else(|| StateStoreError::CorruptState {
                detail: format!("installation has unknown lifecycle '{label}'"),
            })
        })
        .transpose()
}

fn ensure_reconciliation_active(transaction: &Transaction<'_>) -> Result<(), StateStoreError> {
    let lifecycle = load_installation_lifecycle(transaction)?;
    if lifecycle == Some(InstallationLifecycle::Deleting) {
        return Err(StateStoreError::CorruptState {
            detail: "installation deletion has begun; project reconciliation is frozen".to_owned(),
        });
    }

    Ok(())
}

fn exact_path(path: &Path) -> Result<&str, StateStoreError> {
    path.to_str().ok_or_else(|| StateStoreError::NonUtf8Path {
        path: path.to_path_buf(),
    })
}
