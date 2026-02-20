//! Shared doctor output helpers.

use crate::output::{self, LogLevel, Persistence};

pub(super) fn success(message: &str) {
    output::event(
        "doctor",
        LogLevel::Success,
        message,
        Persistence::Persistent,
    );
}

pub(super) fn info(message: &str) {
    output::event("doctor", LogLevel::Info, message, Persistence::Persistent);
}

pub(super) fn error(message: &str) {
    output::event("doctor", LogLevel::Error, message, Persistence::Persistent);
}
