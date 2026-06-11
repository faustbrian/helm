//! Project discovery for daemon watch mode.

use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;

#[derive(Debug, Default)]
pub(crate) struct DiscoveryReport {
    pub(crate) projects: Vec<PathBuf>,
    pub(crate) invalid_configs: Vec<PathBuf>,
    pub(crate) duplicate_projects: Vec<PathBuf>,
}

pub(crate) fn discover_projects(watch_dirs: &[PathBuf]) -> Result<DiscoveryReport> {
    let mut report = DiscoveryReport::default();
    let mut seen = HashSet::new();

    for dir in watch_dirs {
        discover_dir(dir, &mut seen, &mut report)?;
    }

    Ok(report)
}

fn discover_dir(
    dir: &Path,
    seen: &mut HashSet<PathBuf>,
    report: &mut DiscoveryReport,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    let config_path = dir.join(".helm.toml");
    if config_path.exists() {
        match config::load_config_with(config::LoadConfigPathOptions::new(None, Some(dir))) {
            Ok(_) => {
                let project_root = config::project_root_with(config::ProjectRootPathOptions::new(
                    None,
                    Some(dir),
                ))?;
                let canonical = fs::canonicalize(&project_root).unwrap_or(project_root.clone());
                if seen.insert(canonical) {
                    report.projects.push(project_root);
                } else {
                    report.duplicate_projects.push(project_root);
                }
                return Ok(());
            }
            Err(_) => {
                report.invalid_configs.push(dir.to_path_buf());
            }
        }
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        if should_skip_dir(&entry.path()) {
            continue;
        }

        discover_dir(&entry.path(), seen, report)?;
    }

    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(name, ".git" | "node_modules" | "vendor" | "target")
}

#[cfg(test)]
mod tests {
    use super::discover_projects;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "helm-daemon-discovery-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn write_project(root: &PathBuf) {
        fs::create_dir_all(root).expect("create project root");
        fs::write(
            root.join(".helm.toml"),
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("write helm config");
    }

    #[test]
    fn discover_projects_skips_nested_configs_under_valid_project_root() {
        let root = temp_root("nested");
        let parent = root.join("parent");
        let child = parent.join("nested-child");
        write_project(&parent);
        write_project(&child);

        let report = discover_projects(&[root]).expect("discover projects");
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.projects[0], parent);
    }

    #[test]
    fn discover_projects_deduplicates_overlapping_watch_dirs() {
        let root = temp_root("duplicate");
        let project = root.join("project");
        write_project(&project);

        let report = discover_projects(&[root.clone(), project.clone()]).expect("discover");
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.duplicate_projects, vec![project]);
    }

    #[test]
    fn discover_projects_reports_invalid_configs_and_keeps_valid_siblings() {
        let root = temp_root("invalid");
        let invalid = root.join("invalid");
        fs::create_dir_all(&invalid).expect("create invalid dir");
        fs::write(
            invalid.join(".helm.toml"),
            "schema_version = 1\nservice = []\n",
        )
        .expect("write invalid config");

        let valid = root.join("valid");
        write_project(&valid);

        let report = discover_projects(&[root]).expect("discover");
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.invalid_configs, vec![invalid]);
    }
}
