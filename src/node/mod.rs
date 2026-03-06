//! Shared Node toolchain modeling and command resolution.

mod command;
mod package_json;
mod project_files;
mod resolve;
mod types;

pub(crate) use command::{BuildNodeCommandOptions, build_node_command};
pub(crate) use project_files::{detect_node_package_manager, detect_node_version};
pub(crate) use resolve::{ResolveNodeRuntimeOptions, resolve_node_runtime};
pub use types::{NodeToolchain, PackageManager, VersionManager};
