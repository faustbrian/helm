use super::{
    ProjectRecord, ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
    StateStore, StateStoreError,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const CURRENT_SCHEMA_VERSION: u32 = 2;

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

        transaction.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
        transaction.commit()?;

        Ok(())
    }
}

impl StateStore for SqliteStateStore {
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

    fn upsert_resources(&mut self, resources: &[ResourceRecord]) -> Result<(), StateStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        for resource in resources {
            transaction.execute(
                "INSERT INTO resources (\n\
                     resource_id, installation_id, kind, compatibility_fingerprint,\n\
                     project_id, resource_schema_version, desired_revision, retention,\n\
                     lifecycle, orphaned_at_unix_seconds\n\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)\n\
                 ON CONFLICT(resource_id) DO UPDATE SET\n\
                     installation_id = excluded.installation_id,\n\
                     kind = excluded.kind,\n\
                     compatibility_fingerprint = excluded.compatibility_fingerprint,\n\
                     project_id = excluded.project_id,\n\
                     resource_schema_version = excluded.resource_schema_version,\n\
                     desired_revision = excluded.desired_revision,\n\
                     retention = excluded.retention,\n\
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
}

fn exact_path(path: &Path) -> Result<&str, StateStoreError> {
    path.to_str().ok_or_else(|| StateStoreError::NonUtf8Path {
        path: path.to_path_buf(),
    })
}
