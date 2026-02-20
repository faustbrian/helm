//! config paths search module.
//!
//! Contains config paths search logic used by Helm command workflows.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

pub(super) fn find_config_file() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to get current directory")?;

    find_config_in_path(&current_dir)
}

pub(super) fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut current = start;

    loop {
        if current.join(".helm.toml").exists() {
            return Ok(current.to_path_buf());
        }

        current = current.parent().ok_or_else(|| {
            anyhow!(".helm.toml not found in current directory or any parent directory")
        })?;
    }
}

pub(super) fn find_config_in_path(start_path: &Path) -> Result<PathBuf> {
    let mut current = start_path;

    loop {
        let config_path = current.join(".helm.toml");
        if config_path.exists() {
            return Ok(config_path);
        }

        current = current.parent().ok_or_else(|| {
            anyhow!(".helm.toml not found in current directory or any parent directory")
        })?;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{find_config_file, find_config_in_path, find_project_root};

    fn temp_tree() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "helm-path-search-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(dir.join("nested")).expect("create nested dir");
        dir
    }

    #[test]
    fn find_project_root_climbs_until_config_is_found() {
        let root = temp_tree();
        let nested = root.join("nested");
        fs::write(root.join(".helm.toml"), "schema_version = 1\n").expect("seed root config");

        let result = find_project_root(&nested).expect("find root");
        assert_eq!(result, root);
    }

    #[test]
    fn find_config_in_path_supports_direct_match() {
        let root = temp_tree();
        let nested = root.join("nested");
        fs::write(root.join(".helm.toml"), "schema_version = 1\n").expect("seed root config");

        let result = find_config_in_path(&nested).expect("find config from nested");
        assert_eq!(result, root.join(".helm.toml"));
    }

    #[test]
    fn find_config_file_uses_current_directory_when_available() {
        let root = temp_tree();
        let cwd = std::env::current_dir().expect("capture cwd");
        let expected = root.join(".helm.toml");
        fs::write(&expected, "schema_version = 1\n").expect("seed config in temp");

        std::env::set_current_dir(&root).expect("set cwd to temp");
        let found = find_config_file().expect("find config in cwd");
        std::env::set_current_dir(&cwd).expect("restore cwd");

        assert_eq!(
            found.canonicalize().expect("canonicalize found"),
            expected.canonicalize().expect("canonicalize expected")
        );
    }
}
