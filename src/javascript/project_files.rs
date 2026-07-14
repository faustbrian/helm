use std::path::Path;

use super::PackageManager;
use super::package_json::read_package_json;

pub(crate) fn detect_node_package_manager(workspace_root: &Path) -> Option<PackageManager> {
    detect_package_json_package_manager(workspace_root)
        .or_else(|| detect_lockfile_package_manager(workspace_root))
}

fn detect_package_json_package_manager(workspace_root: &Path) -> Option<PackageManager> {
    let package_json = read_package_json(workspace_root)?;
    let package_manager = package_json.get("packageManager")?.as_str()?;
    parse_package_manager_name(package_manager)
}

fn detect_lockfile_package_manager(workspace_root: &Path) -> Option<PackageManager> {
    [
        ("pnpm-lock.yaml", PackageManager::Pnpm),
        ("yarn.lock", PackageManager::Yarn),
        ("package-lock.json", PackageManager::Npm),
        ("npm-shrinkwrap.json", PackageManager::Npm),
    ]
    .into_iter()
    .find_map(|(file_name, manager)| workspace_root.join(file_name).is_file().then_some(manager))
}

fn parse_package_manager_name(value: &str) -> Option<PackageManager> {
    match package_manager_name(value) {
        "npm" => Some(PackageManager::Npm),
        "pnpm" => Some(PackageManager::Pnpm),
        "yarn" => Some(PackageManager::Yarn),
        _ => None,
    }
}

fn package_manager_name(value: &str) -> &str {
    value.split('@').next().unwrap_or(value)
}
