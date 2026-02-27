//! Swarm orchestration runner.
//!
//! The runner resolves target sets, builds per-target child invocations, executes
//! them serially or in parallel, and emits a single aggregate summary/failure.

mod api;
mod args;
mod execution;
mod output;
mod planning;
mod project_deps;
mod run;
mod summary;

#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use api::swarm_child_args;
pub(crate) use api::{
    RunProjectSwarmDependenciesOptions, RunSwarmOptions, run_project_swarm_dependencies, run_swarm,
};
