//! cli hooks module.
//!
//! Contains lifecycle hook execution logic.

mod run_phase;

pub(crate) use run_phase::run_phase_hooks_for_services;
