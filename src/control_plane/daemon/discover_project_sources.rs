use super::{
    ProjectDiscoveryError, ProjectDiscoveryIssue, ProjectDiscoveryOptions, ProjectDiscoveryReport,
};
use crate::control_plane::application::ProjectSource;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::Read;
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
    let mut entry_count = 0_usize;

    while let Some((directory, depth)) = pending.pop_front() {
        if !visited_directories.insert(directory.clone()) {
            continue;
        }
        inspect_project_directory(&directory, options, &mut sources, &mut issues)?;
        let mut entries = fs::read_dir(&directory)
            .map_err(|source| ProjectDiscoveryError::Io {
                action: "read watched directory",
                path: directory.clone(),
                source,
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| ProjectDiscoveryError::Io {
                action: "read watched directory entry",
                path: directory.clone(),
                source,
            })?;
        entries.sort_by_key(fs::DirEntry::file_name);

        for entry in entries {
            entry_count = entry_count.saturating_add(1);
            if entry_count > options.maximum_entries() {
                return Err(ProjectDiscoveryError::EntryLimit {
                    maximum: options.maximum_entries(),
                });
            }
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|source| ProjectDiscoveryError::Io {
                    action: "inspect watched directory entry",
                    path: path.clone(),
                    source,
                })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            if PRUNED_DIRECTORIES
                .iter()
                .any(|name| entry.file_name() == *name)
            {
                continue;
            }
            if depth >= options.maximum_depth() {
                issues.push(ProjectDiscoveryIssue::DepthLimit {
                    path,
                    maximum: options.maximum_depth(),
                });
            } else {
                pending.push_back((path, depth + 1));
            }
        }
    }

    issues.sort_by(|first, second| first.to_string().cmp(&second.to_string()));

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
) -> Result<(), ProjectDiscoveryError> {
    let config_path = directory.join(CONFIG_FILE);
    match fs::symlink_metadata(&config_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            issues.push(ProjectDiscoveryIssue::SymlinkConfig { path: config_path });
            return Ok(());
        }
        Ok(metadata) if metadata.is_file() => {
            if metadata.len() > options.maximum_config_bytes() as u64 {
                issues.push(ProjectDiscoveryIssue::ConfigTooLarge {
                    path: config_path,
                    actual: metadata.len(),
                    maximum: options.maximum_config_bytes(),
                });
                return Ok(());
            }
            match read_bounded_utf8(&config_path, options.maximum_config_bytes()) {
                Ok(yaml) => {
                    let mut source = ProjectSource::new(directory.to_path_buf(), config_path, yaml);
                    if let Some((lock_path, lock_yaml)) =
                        read_optional_artifact_lock(directory, options, issues)?
                    {
                        source = source.with_artifact_lock(lock_path, lock_yaml);
                    }
                    sources.insert(directory.to_path_buf(), source);
                }
                Err(issue) => issues.push(issue),
            }
            return Ok(());
        }
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ProjectDiscoveryError::Io {
                action: "inspect project config",
                path: config_path,
                source,
            });
        }
    }

    Ok(())
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

    match read_bounded_artifact_lock(&path, options.maximum_config_bytes()) {
        Ok(yaml) => Ok(Some((path, yaml))),
        Err(issue) => {
            issues.push(issue);
            Ok(None)
        }
    }
}

fn read_bounded_artifact_lock(
    path: &Path,
    maximum: usize,
) -> Result<String, ProjectDiscoveryIssue> {
    let file =
        fs::File::open(path).map_err(|error| ProjectDiscoveryIssue::UnreadableArtifactLock {
            path: path.to_path_buf(),
            detail: error.to_string(),
        })?;
    let limit = u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1);
    let mut bytes = Vec::with_capacity(maximum.min(64 * 1024));
    file.take(limit).read_to_end(&mut bytes).map_err(|error| {
        ProjectDiscoveryIssue::UnreadableArtifactLock {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
    })?;
    if bytes.len() > maximum {
        return Err(ProjectDiscoveryIssue::ArtifactLockTooLarge {
            path: path.to_path_buf(),
            actual: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            maximum,
        });
    }

    String::from_utf8(bytes).map_err(|error| ProjectDiscoveryIssue::UnreadableArtifactLock {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}

fn read_bounded_utf8(path: &Path, maximum: usize) -> Result<String, ProjectDiscoveryIssue> {
    let file = fs::File::open(path).map_err(|error| ProjectDiscoveryIssue::UnreadableConfig {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })?;
    let limit = u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1);
    let mut bytes = Vec::with_capacity(maximum.min(64 * 1024));
    file.take(limit).read_to_end(&mut bytes).map_err(|error| {
        ProjectDiscoveryIssue::UnreadableConfig {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
    })?;
    if bytes.len() > maximum {
        return Err(ProjectDiscoveryIssue::ConfigTooLarge {
            path: path.to_path_buf(),
            actual: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            maximum,
        });
    }

    String::from_utf8(bytes).map_err(|error| ProjectDiscoveryIssue::UnreadableConfig {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}
