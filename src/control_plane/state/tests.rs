use super::{ProjectRecord, SqliteStateStore, StateStore};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn opening_a_new_store_applies_the_current_schema_atomically() {
    let database_path = temporary_database_path("migration");

    let store = SqliteStateStore::open(&database_path).expect("open state store");

    assert_eq!(store.schema_version().expect("schema version"), 1);
    assert_eq!(store.journal_mode().expect("journal mode"), "wal");

    drop(store);
    remove_database(&database_path);
}

#[test]
fn project_ownership_survives_store_restart() {
    let database_path = temporary_database_path("restart");
    let record = project_record(
        "/work/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .replace_project(&record)
            .expect("persist project ownership");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(store.projects().expect("load projects"), vec![record]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn conflicting_route_ownership_rolls_back_the_entire_project_write() {
    let database_path = temporary_database_path("collision");
    let first = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let conflicting = project_record(
        "/work/archive/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_project(&first)
        .expect("persist first project");

    let error = store
        .replace_project(&conflicting)
        .expect_err("route ownership conflict");

    assert_eq!(
        error.to_string(),
        "route 'bill-app.stackctl.localhost' is owned by '/work/bill', not '/work/archive/bill'"
    );
    assert_eq!(store.projects().expect("load projects"), vec![first]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn dropping_an_uncommitted_transaction_leaves_no_partial_project() {
    let database_path = temporary_database_path("interruption");

    {
        let store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .connection
            .execute_batch(
                "BEGIN IMMEDIATE;\n\
                 INSERT INTO projects (canonical_path, project_name)\n\
                 VALUES ('/work/partial', 'partial');",
            )
            .expect("write uncommitted project");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert!(store.projects().expect("load projects").is_empty());

    drop(store);
    remove_database(&database_path);
}

fn project_record(path: &str, name: &str, domains: &[&str]) -> ProjectRecord {
    ProjectRecord::new(
        PathBuf::from(path),
        name.to_owned(),
        domains.iter().map(|domain| (*domain).to_owned()).collect(),
    )
}

fn temporary_database_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "stackctl-v8-{name}-{}-{unique}.sqlite3",
        std::process::id()
    ))
}

fn remove_database(database_path: &Path) {
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove temporary state database");
        }
    }
}
