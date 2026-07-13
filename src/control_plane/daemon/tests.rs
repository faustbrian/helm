use super::SingletonLease;
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

fn remove_lock(lock_path: &Path) {
    if lock_path.exists() {
        std::fs::remove_file(lock_path).expect("remove singleton lease");
    }
}
