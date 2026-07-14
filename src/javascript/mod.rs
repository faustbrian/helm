//! V8 project package-manager detection for containerized commands.

mod package_json;
mod project_files;
mod types;

pub(crate) use project_files::detect_node_package_manager;
pub use types::PackageManager;
