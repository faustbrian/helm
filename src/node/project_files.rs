use std::path::Path;

use super::package_json::read_package_json;
use super::{PackageManager, VersionManager};

pub(crate) fn detect_node_package_manager(workspace_root: &Path) -> Option<PackageManager> {
    detect_package_json_package_manager(workspace_root)
        .or_else(|| detect_lockfile_package_manager(workspace_root))
}

pub(crate) fn detect_package_json_package_manager(workspace_root: &Path) -> Option<PackageManager> {
    let package_json = read_package_json(workspace_root)?;
    let package_manager = package_json.get("packageManager")?.as_str()?;
    parse_package_manager_name(package_manager)
}

pub(crate) fn detect_node_version(workspace_root: &Path) -> Option<String> {
    detect_package_json_volta_node_version(workspace_root)
        .or_else(|| read_version_file(&workspace_root.join(".nvmrc")))
        .or_else(|| read_version_file(&workspace_root.join(".node-version")))
        .or_else(|| detect_package_json_engines_node_version(workspace_root))
}

pub(crate) fn detect_package_json_volta_node_version(workspace_root: &Path) -> Option<String> {
    let package_json = read_package_json(workspace_root)?;
    package_json
        .get("volta")?
        .get("node")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn detect_package_json_engines_node_version(workspace_root: &Path) -> Option<String> {
    let package_json = read_package_json(workspace_root)?;
    package_json
        .get("engines")?
        .get("node")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn detect_lockfile_package_manager(workspace_root: &Path) -> Option<PackageManager> {
    [
        ("bun.lock", PackageManager::Bun),
        ("bun.lockb", PackageManager::Bun),
        ("pnpm-lock.yaml", PackageManager::Pnpm),
        ("yarn.lock", PackageManager::Yarn),
        ("package-lock.json", PackageManager::Npm),
        ("npm-shrinkwrap.json", PackageManager::Npm),
    ]
    .into_iter()
    .find_map(|(file_name, manager)| workspace_root.join(file_name).is_file().then_some(manager))
}

fn read_version_file(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let value = content.lines().next()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_package_manager_name(value: &str) -> Option<PackageManager> {
    let name = value.split('@').next().unwrap_or(value);
    match name {
        "bun" => Some(PackageManager::Bun),
        "npm" => Some(PackageManager::Npm),
        "pnpm" => Some(PackageManager::Pnpm),
        "yarn" => Some(PackageManager::Yarn),
        _ => None,
    }
}

#[allow(dead_code)]
pub(crate) fn default_version_manager() -> VersionManager {
    VersionManager::System
}
