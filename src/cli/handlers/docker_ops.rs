//! cli handlers docker ops module.
//!
//! Contains docker passthrough command handlers used by Helm command workflows.

pub(super) use super::service_scope::{
    for_each_service as run_for_each_docker_service,
    for_selected_services as run_for_selected_docker_services,
    selected_services as selected_docker_services,
};
pub(crate) use attach::{HandleAttachOptions, handle_attach};
pub(crate) use cp::{HandleCpOptions, handle_cp};
pub(crate) use events::{HandleEventsOptions, handle_events};
pub(crate) use inspect::{HandleInspectOptions, handle_inspect};
pub(crate) use kill::handle_kill;
pub(crate) use pause::handle_pause;
pub(crate) use port::handle_port;
pub(crate) use prune::{HandlePruneOptions, handle_prune};
pub(crate) use stats::handle_stats;
pub(crate) use top::handle_top;
pub(crate) use unpause::handle_unpause;
pub(crate) use wait::handle_wait;

mod attach;
mod cp;
mod events;
mod inspect;
mod kill;
mod log;
mod output_json;
mod pause;
mod port;
mod prune;
mod stats;
mod top;
mod unpause;
mod wait;
