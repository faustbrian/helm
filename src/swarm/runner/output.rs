//! Shared output-mode aware emission helpers for swarm runner.

use crate::output::{self, LogLevel, Persistence};
use crate::swarm::target_exec::OutputMode;

pub(super) fn emit_swarm(mode: OutputMode, level: LogLevel, message: &str) {
    match mode {
        OutputMode::Logged => output::event("swarm", level, message, Persistence::Persistent),
        OutputMode::Passthrough => {
            if matches!(level, LogLevel::Error | LogLevel::Warn) {
                eprintln!("{message}");
            } else {
                println!("{message}");
            }
        }
    }
}
