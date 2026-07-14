use super::Cli;

use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unsupported_toml_project() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "stackctl-main-test-dispatch-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).expect("create temporary project root");
    fs::write(
        root.join(".stackctl.toml"),
        "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
    )
    .expect("write unsupported project config");
    root
}

#[test]
fn cli_dispatch_rejects_pre_v8_project_config() {
    let project_root = unsupported_toml_project();
    crate::docker::with_test_runtime_lock(|| {
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            project_root.to_str().expect("project root path"),
            "status",
        ]);

        let error = crate::cli::dispatch::run(cli).expect_err("pre-v8 config is unsupported");
        assert!(error.to_string().contains("pre-v8 config"));
        assert!(error.to_string().contains("clean v8 installation"));
    });
}

#[test]
fn cli_dispatch_daemon_status_works_without_local_project_context() {
    crate::docker::with_test_runtime_lock(|| {
        let cli = Cli::parse_from(["stackctl", "daemon", "status"]);

        let error = crate::cli::dispatch::run(cli).expect_err("daemon is not running in test");
        assert!(!error.to_string().contains("configuration"));
    });
}
