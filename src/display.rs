//! Display formatting for CLI-facing summaries.

#![allow(clippy::print_stdout)] // Display functions intentionally print to stdout

mod about;
mod about_style;
mod status;

pub(crate) use about::print_about;
pub use status::print_status;
