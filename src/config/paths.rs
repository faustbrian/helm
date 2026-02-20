//! config paths module.
//!
//! Contains config paths logic used by Helm command workflows.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

mod search;
mod template;

use search::{find_config_file, find_config_in_path, find_project_root};
use template::default_config_template;

pub(super) fn project_root() -> Result<PathBuf> {
    project_root_with(None, None)
}

pub(super) fn project_root_with(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = config_path {
        return path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("config path '{}' has no parent directory", path.display()));
    }

    if let Some(root) = project_root {
        return find_project_root(root);
    }

    let current_dir = std::env::current_dir().context("failed to get current directory")?;
    find_project_root(&current_dir)
}

pub(super) fn init_config() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to get current directory")?;
    let config_path = current_dir.join(".helm.toml");

    if config_path.exists() {
        anyhow::bail!(".helm.toml already exists in {}", current_dir.display());
    }

    let project_name = current_dir
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("my-app");
    std::fs::write(&config_path, default_config_template(project_name))
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    Ok(config_path)
}

/// Resolves config path using configured inputs and runtime state.
pub(super) fn resolve_config_path(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = config_path {
        if !path.exists() {
            anyhow::bail!("config file not found at {}", path.display());
        }
        return Ok(path.to_path_buf());
    }

    if let Some(root) = project_root {
        return find_config_in_path(root);
    }

    find_config_file()
}

/// Resolves lockfile path using configured inputs and runtime state.
pub(super) fn resolve_lockfile_path(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    Ok(project_root_with(config_path, project_root)?.join(".helm.lock.toml"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        init_config, project_root, project_root_with, resolve_config_path, resolve_lockfile_path,
    };

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "helm-config-paths-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn project_root_with_prefers_config_path_parent() {
        let root = temp_root();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested");
        fs::write(root.join(".helm.toml"), "schema_version = 1\n").expect("seed config");
        let config_path = nested.join("custom.toml");

        let found = project_root_with(Some(&config_path), None).expect("find from config path");
        assert_eq!(found, nested);
    }

    #[test]
    fn project_root_with_finds_ancestor_config_for_project_root() {
        let root = temp_root();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested");
        fs::write(root.join(".helm.toml"), "schema_version = 1\n").expect("seed config");

        let found = project_root_with(None, Some(&nested)).expect("find from project root");
        assert_eq!(found, root);
    }

    #[test]
    fn project_root_proxies_to_with_default_path_lookup() {
        let root = temp_root();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested");
        fs::write(root.join(".helm.toml"), "schema_version = 1\n").expect("seed config");

        let cwd = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(&nested).expect("set nested cwd");
        let found = project_root().expect("find project root from cwd");
        std::env::set_current_dir(cwd).expect("restore cwd");

        assert_eq!(
            found.canonicalize().expect("found canonical"),
            root.canonicalize().expect("root canonical")
        );
    }

    #[test]
    fn init_config_creates_default_file() {
        let root = temp_root();
        let cwd = std::env::current_dir().expect("current dir");
        let original = root.join(".helm.toml");

        let found = {
            std::env::set_current_dir(&root).expect("set cwd");
            let config_path = init_config().expect("init config");
            std::env::set_current_dir(cwd).expect("restore cwd");
            config_path
        };

        assert!(original.exists());
        assert_eq!(
            found.canonicalize().expect("found canonical"),
            original.canonicalize().expect("original canonical")
        );
    }

    #[test]
    fn init_config_rejects_existing_file() {
        let root = temp_root();
        let existing = root.join(".helm.toml");
        fs::write(&existing, "schema_version = 1\n").expect("seed existing");

        let cwd = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(&root).expect("set cwd");
        let result = init_config();
        std::env::set_current_dir(cwd).expect("restore cwd");

        assert!(result.is_err());
    }

    #[test]
    fn resolve_config_path_prefers_explicit_path() {
        let root = temp_root();
        let config_path = root.join("custom.toml");
        fs::write(&config_path, "schema_version = 1\n").expect("seed config");

        let resolved =
            resolve_config_path(Some(&config_path), None).expect("resolve explicit path");
        assert_eq!(resolved, config_path);
    }

    #[test]
    fn resolve_lockfile_path_uses_project_root_context() {
        let root = temp_root();
        fs::write(root.join(".helm.toml"), "schema_version = 1\n").expect("seed config");
        let expected = root.join(".helm.lock.toml");

        let resolved = resolve_lockfile_path(None, Some(&root)).expect("resolve lock path");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn project_root_with_errors_when_no_config_is_found() {
        let root = temp_root();
        let error = project_root_with(None, Some(&root)).unwrap_err();
        assert!(error.to_string().contains(".helm.toml not found"));
    }
}
