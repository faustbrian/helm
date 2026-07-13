use super::{ProjectDiscoveryOptions, SingletonLease, discover_project_sources};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn only_one_daemon_can_hold_a_user_lease() {
    let lock_path = temporary_lock_path("exclusive");
    let first = SingletonLease::acquire(&lock_path).expect("first singleton lease");

    let error = SingletonLease::acquire(&lock_path).expect_err("second lease must fail");

    assert_eq!(
        error.to_string(),
        format!("another Stackctl daemon owns '{}'", lock_path.display())
    );

    drop(first);
    remove_lock(&lock_path);
}

#[test]
fn released_daemon_lease_can_be_acquired_without_stale_pid_recovery() {
    let lock_path = temporary_lock_path("recovery");

    {
        let _first = SingletonLease::acquire(&lock_path).expect("first singleton lease");
    }

    let recovered = SingletonLease::acquire(&lock_path).expect("recovered singleton lease");
    let owner = std::fs::read_to_string(&lock_path).expect("read lease owner");

    assert_eq!(owner, std::process::id().to_string());

    drop(recovered);
    remove_lock(&lock_path);
}

#[cfg(unix)]
#[test]
fn daemon_lease_is_readable_and_writable_only_by_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let lock_path = temporary_lock_path("permissions");
    let lease = SingletonLease::acquire(&lock_path).expect("singleton lease");
    let mode = std::fs::metadata(&lock_path)
        .expect("lease metadata")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(mode, 0o600);

    drop(lease);
    remove_lock(&lock_path);
}

#[test]
fn watched_root_scan_discovers_nested_yaml_in_canonical_order() {
    let root = temporary_directory("discovery");
    let alpha = root.join("alpha");
    let zeta = root.join("nested/zeta");
    std::fs::create_dir_all(&alpha).expect("alpha directory");
    std::fs::create_dir_all(&zeta).expect("zeta directory");
    std::fs::write(
        alpha.join(".stackctl.yaml"),
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("alpha config");
    std::fs::write(
        zeta.join(".stackctl.yaml"),
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("zeta config");

    let report = discover_project_sources(
        &[root.clone(), root.clone()],
        ProjectDiscoveryOptions::bounded_defaults(),
    )
    .expect("project discovery");
    let sources = report.sources();

    assert_eq!(sources.len(), 2);
    assert!(report.issues().is_empty());
    assert_eq!(
        sources
            .iter()
            .map(|source| source.canonical_path())
            .collect::<Vec<_>>(),
        vec![
            alpha.canonicalize().expect("canonical alpha"),
            zeta.canonicalize().expect("canonical zeta"),
        ]
    );

    std::fs::remove_dir_all(&root).expect("remove discovery fixture");
}

#[test]
fn watched_root_scan_reports_all_toml_only_projects_without_loading_toml() {
    let root = temporary_directory("legacy-toml");
    for project in ["alpha", "zeta"] {
        let directory = root.join(project);
        std::fs::create_dir_all(&directory).expect("project directory");
        std::fs::write(directory.join(".stackctl.toml"), "project = 'legacy'\n")
            .expect("legacy config");
    }
    let valid = root.join("valid");
    std::fs::create_dir(&valid).expect("valid project directory");
    std::fs::write(
        valid.join(".stackctl.yaml"),
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("valid YAML config");

    let report = discover_project_sources(
        std::slice::from_ref(&root),
        ProjectDiscoveryOptions::bounded_defaults(),
    )
    .expect("bounded discovery");
    let diagnostics = report
        .issues()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(report.sources().len(), 1);
    assert_eq!(report.issues().len(), 2);
    assert!(diagnostics.contains("alpha/.stackctl.toml"));
    assert!(diagnostics.contains("zeta/.stackctl.toml"));
    assert!(diagnostics.contains("stackctl config migrate --to yaml"));

    std::fs::remove_dir_all(&root).expect("remove TOML fixture");
}

#[cfg(unix)]
#[test]
fn watched_root_scan_never_follows_a_symlinked_project_config() {
    use std::os::unix::fs::symlink;

    let root = temporary_directory("symlink-config");
    let target = root.join("outside.yaml");
    std::fs::write(
        &target,
        "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("symlink target");
    symlink(&target, root.join(".stackctl.yaml")).expect("config symlink");

    let report = discover_project_sources(
        std::slice::from_ref(&root),
        ProjectDiscoveryOptions::bounded_defaults(),
    )
    .expect("bounded discovery");

    assert!(report.sources().is_empty());
    assert_eq!(report.issues().len(), 1);
    assert!(report.issues()[0].to_string().contains("symbolic link"));

    std::fs::remove_dir_all(&root).expect("remove symlink fixture");
}

#[test]
fn watched_root_scan_is_bounded_before_reading_oversized_configuration() {
    let root = temporary_directory("oversized-config");
    std::fs::write(root.join(".stackctl.yaml"), vec![b'x'; 129]).expect("oversized config");
    let options = ProjectDiscoveryOptions::new(8, 100, 128).expect("discovery options");

    let report =
        discover_project_sources(std::slice::from_ref(&root), options).expect("bounded discovery");

    assert!(report.sources().is_empty());
    assert!(
        report.issues()[0]
            .to_string()
            .contains("is 129 bytes; maximum is 128 bytes")
    );

    std::fs::remove_dir_all(&root).expect("remove oversized fixture");
}

fn temporary_lock_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "stackctl-v8-{name}-{}-{unique}.lock",
        std::process::id()
    ))
}

fn temporary_directory(name: &str) -> PathBuf {
    let path = temporary_lock_path(name).with_extension("directory");
    std::fs::create_dir(&path).expect("temporary directory");
    path
}

fn remove_lock(lock_path: &Path) {
    if lock_path.exists() {
        std::fs::remove_file(lock_path).expect("remove singleton lease");
    }
}
