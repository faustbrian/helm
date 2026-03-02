//! cli hooks module.
//!
//! Contains lifecycle hook execution logic.

mod run_phase;
mod selection;

pub(crate) use run_phase::run_phase_hooks_for_services;
pub(crate) use selection::run_hooks_for_up_selection;
