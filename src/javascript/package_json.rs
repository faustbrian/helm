use serde_json::Value;
use std::path::Path;

pub(super) fn read_package_json(workspace_root: &Path) -> Option<Value> {
    let path = workspace_root.join("package.json");
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}
