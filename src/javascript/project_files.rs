use std::path::Path;

use super::package_json::read_package_json;
use super::{JavaScriptRuntime, PackageManager, VersionManager};

pub(crate) fn detect_javascript_runtime(workspace_root: &Path) -> Option<JavaScriptRuntime> {
    if [
        workspace_root.join("deno.json"),
        workspace_root.join("deno.jsonc"),
        workspace_root.join("deno.lock"),
    ]
    .into_iter()
    .any(|path| path.is_file())
    {
        return Some(JavaScriptRuntime::Deno);
    }

    if detect_bun_runtime(workspace_root) {
        return Some(JavaScriptRuntime::Bun);
    }

    detect_node_package_manager(workspace_root)
        .is_some()
        .then_some(JavaScriptRuntime::Node)
}

pub(crate) fn detect_node_package_manager(workspace_root: &Path) -> Option<PackageManager> {
    detect_package_json_package_manager(workspace_root)
        .or_else(|| detect_lockfile_package_manager(workspace_root))
}

fn detect_bun_runtime(workspace_root: &Path) -> bool {
    workspace_root.join("bun.lock").is_file()
        || workspace_root.join("bun.lockb").is_file()
        || package_json_declares_bun(workspace_root)
}

fn package_json_declares_bun(workspace_root: &Path) -> bool {
    read_package_json(workspace_root)
        .and_then(|package_json| {
            package_json
                .get("packageManager")?
                .as_str()
                .map(str::to_owned)
        })
        .map(|package_manager| package_manager_name(&package_manager) == "bun")
        .unwrap_or(false)
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

#[allow(dead_code)]
pub(crate) fn default_version_manager() -> VersionManager {
    VersionManager::System
}
