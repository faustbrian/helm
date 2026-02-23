//! Shared docker-ops logging helpers.

use crate::output::{self, LogLevel, Persistence};

pub(super) fn info(message: &str) {
    output::event("docker", LogLevel::Info, message, Persistence::Persistent);
}

pub(super) fn warn(message: &str) {
    output::event("docker", LogLevel::Warn, message, Persistence::Persistent);
}

#[cfg(test)]
mod tests {
    use super::{info, warn};

    #[test]
    fn invokes_info_and_warn_logging_helpers() {
        info("starting");
        warn("stopping");
    }
}
