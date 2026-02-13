//! cli support open in browser module.
//!
//! Contains cli support open in browser logic used by Helm command workflows.

use std::process::Command;

pub(crate) fn open_in_browser(url: &str) {
    if cfg!(target_os = "macos") {
        drop(Command::new("open").arg(url).status());
    } else {
        drop(Command::new("xdg-open").arg(url).status());
    }
}
