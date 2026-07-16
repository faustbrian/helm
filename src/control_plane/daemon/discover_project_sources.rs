use super::{
    ProjectDiscoveryError, ProjectDiscoveryIssue, ProjectDiscoveryOptions, ProjectDiscoveryReport,
};
use crate::control_plane::{application::ProjectSource, read_bounded_yaml_file};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = ".stackctl.yaml";
const ARTIFACT_LOCK_FILE: &str = ".stackctl.lock.yaml";
const PRUNED_DIRECTORIES: [&str; 5] = [".git", ".stackctl", "node_modules", "target", "vendor"];

/// Performs one bounded correctness scan without executing project code.
pub(crate) fn discover_project_sources(
    roots: &[PathBuf],
    options: ProjectDiscoveryOptions,
) -> Result<ProjectDiscoveryReport, ProjectDiscoveryError> {
    let mut canonical_roots = BTreeSet::new();
    for root in roots {
        let canonical = fs::canonicalize(root).map_err(|source| ProjectDiscoveryError::Io {
            action: "canonicalize watched root",
            path: root.clone(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(ProjectDiscoveryError::RootNotDirectory { path: canonical });
        }
        canonical_roots.insert(canonical);
    }

    let mut pending = canonical_roots
        .into_iter()
        .map(|path| (path, 0_usize))
        .collect::<VecDeque<_>>();
    let mut sources = BTreeMap::new();
    let mut issues = Vec::new();
    let mut visited_directories = BTreeSet::new();
    let mut directory_count = 0_usize;

    while let Some((directory, depth)) = pending.pop_front() {
        if !visited_directories.insert(directory.clone()) {
            continue;
        }
        if inspect_project_directory(&directory, options, &mut sources, &mut issues)? {
            continue;
        }
        if depth >= options.maximum_depth() {
            continue;
        }
        let entries = fs::read_dir(&directory).map_err(|source| ProjectDiscoveryError::Io {
            action: "read watched directory",
            path: directory.clone(),
            source,
        })?;
        let mut child_directories = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| ProjectDiscoveryError::Io {
                action: "read watched directory entry",
                path: directory.clone(),
                source,
            })?;
            let file_name = entry.file_name();
            if file_name.as_encoded_bytes().first() == Some(&b'.') {
                continue;
            }
            if PRUNED_DIRECTORIES.iter().any(|name| file_name == *name) {
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|source| ProjectDiscoveryError::Io {
                    action: "inspect watched directory entry type",
                    path: entry.path(),
                    source,
                })?;
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            directory_count = directory_count.saturating_add(1);
            if directory_count > options.maximum_directories() {
                return Err(ProjectDiscoveryError::DirectoryLimit {
                    maximum: options.maximum_directories(),
                });
            }
            child_directories.push(entry.path());
        }
        child_directories.sort();
        for path in child_directories {
            pending.push_back((path, depth + 1));
        }
    }

    issues.sort_by_key(|first| first.to_string());

    Ok(ProjectDiscoveryReport::new(
        sources.into_values().collect(),
        issues,
    ))
}

fn inspect_project_directory(
    directory: &Path,
    options: ProjectDiscoveryOptions,
    sources: &mut BTreeMap<PathBuf, ProjectSource>,
    issues: &mut Vec<ProjectDiscoveryIssue>,
) -> Result<bool, ProjectDiscoveryError> {
    let config_path = directory.join(CONFIG_FILE);
    match fs::symlink_metadata(&config_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            issues.push(ProjectDiscoveryIssue::SymlinkConfig { path: config_path });
            return Ok(true);
        }
        Ok(metadata) if metadata.is_file() => {
            if metadata.len() > options.maximum_config_bytes() as u64 {
                issues.push(ProjectDiscoveryIssue::ConfigTooLarge {
                    path: config_path,
                    actual: metadata.len(),
                    maximum: options.maximum_config_bytes(),
                });
                return Ok(true);
            }
            match read_bounded_yaml_file(&config_path, options.maximum_config_bytes()) {
                Ok(yaml) => {
                    let mut source = ProjectSource::new(directory.to_path_buf(), config_path, yaml);
                    if let Some((lock_path, lock_yaml)) =
                        read_optional_artifact_lock(directory, options, issues)?
                    {
                        source = source.with_artifact_lock(lock_path, lock_yaml);
                    }
                    sources.insert(directory.to_path_buf(), source);
                }
                Err(error) => issues.push(ProjectDiscoveryIssue::UnreadableConfig {
                    path: config_path,
                    detail: error.to_string(),
                }),
            }
            return Ok(true);
        }
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ProjectDiscoveryError::Io {
                action: "inspect project config",
                path: config_path,
                source,
            });
        }
    }

    Ok(false)
}

fn read_optional_artifact_lock(
    directory: &Path,
    options: ProjectDiscoveryOptions,
    issues: &mut Vec<ProjectDiscoveryIssue>,
) -> Result<Option<(PathBuf, String)>, ProjectDiscoveryError> {
    let path = directory.join(ARTIFACT_LOCK_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ProjectDiscoveryError::Io {
                action: "inspect project artifact lock",
                path,
                source,
            });
        }
    };

    if metadata.file_type().is_symlink() {
        issues.push(ProjectDiscoveryIssue::SymlinkArtifactLock { path });
        return Ok(None);
    }
    if !metadata.is_file() {
        issues.push(ProjectDiscoveryIssue::UnreadableArtifactLock {
            path,
            detail: "expected a regular file".to_owned(),
        });
        return Ok(None);
    }
    if metadata.len() > options.maximum_config_bytes() as u64 {
        issues.push(ProjectDiscoveryIssue::ArtifactLockTooLarge {
            path,
            actual: metadata.len(),
            maximum: options.maximum_config_bytes(),
        });
        return Ok(None);
    }

    match read_bounded_yaml_file(&path, options.maximum_config_bytes()) {
        Ok(yaml) => Ok(Some((path, yaml))),
        Err(error) => {
            issues.push(ProjectDiscoveryIssue::UnreadableArtifactLock {
                path,
                detail: error.to_string(),
            });
            Ok(None)
        }
    }
}
