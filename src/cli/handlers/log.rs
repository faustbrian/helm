//! Shared handler logging helpers.

use crate::output::{self, LogLevel, Persistence};

pub(super) fn info_if_not_quiet(quiet: bool, scope: &str, message: &str) {
    if quiet {
        return;
    }
    output::event(scope, LogLevel::Info, message, Persistence::Persistent);
}

pub(super) fn success_if_not_quiet(quiet: bool, scope: &str, message: &str) {
    if quiet {
        return;
    }
    output::event(scope, LogLevel::Success, message, Persistence::Persistent);
}
