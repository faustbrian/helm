//! cli module.
//!
//! Contains cli logic used by Stackctl command workflows.

pub mod args;
#[path = "cli/support/open_in_browser.rs"]
pub(crate) mod browser_opener;
pub mod dispatch;
pub mod handlers;
