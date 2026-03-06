//! Shared JavaScript toolchain modeling and command resolution.

mod command;
mod package_json;
mod project_files;
mod resolve;
mod types;

pub(crate) use command::{BuildNodeCommandOptions, build_node_command};
pub(crate) use project_files::{
    detect_javascript_runtime, detect_node_package_manager, detect_node_version,
};
pub(crate) use resolve::{ResolveJavaScriptRuntimeOptions, resolve_javascript_runtime};
pub use types::{JavaScriptRuntime, JavaScriptToolchain, PackageManager, VersionManager};
