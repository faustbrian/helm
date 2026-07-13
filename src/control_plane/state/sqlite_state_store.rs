use super::{ProjectRecord, StateStore, StateStoreError};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::{Path, PathBuf};

const CURRENT_SCHEMA_VERSION: u32 = 1;

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
                 ON route_claims(canonical_path);\n\
             PRAGMA user_version = 1;",
        )?;
        transaction.commit()?;

        Ok(())
    }
}

impl StateStore for SqliteStateStore {
    fn replace_project(&mut self, project: &ProjectRecord) -> Result<(), StateStoreError> {
        let canonical_path = exact_path(project.canonical_path())?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        for domain in project.route_domains() {
            let existing_path = transaction
                .query_row(
                    "SELECT canonical_path FROM route_claims WHERE domain = ?1",
                    [domain],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            if let Some(existing_path) = existing_path {
                if existing_path != canonical_path {
                    return Err(StateStoreError::RouteOwnershipConflict {
                        domain: domain.clone(),
                        existing_path: PathBuf::from(existing_path),
                        requested_path: project.canonical_path().to_path_buf(),
                    });
                }
            }
        }

        transaction.execute(
            "INSERT INTO projects (canonical_path, project_name) VALUES (?1, ?2)\n\
             ON CONFLICT(canonical_path) DO UPDATE SET project_name = excluded.project_name",
            params![canonical_path, project.project_name()],
        )?;
        transaction.execute(
            "DELETE FROM route_claims WHERE canonical_path = ?1",
            [canonical_path],
        )?;

        for domain in project.route_domains() {
            transaction.execute(
                "INSERT INTO route_claims (domain, canonical_path) VALUES (?1, ?2)",
                params![domain, canonical_path],
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
}

fn exact_path(path: &Path) -> Result<&str, StateStoreError> {
    path.to_str().ok_or_else(|| StateStoreError::NonUtf8Path {
        path: path.to_path_buf(),
    })
}
