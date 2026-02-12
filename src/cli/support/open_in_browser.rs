use std::process::Command;

pub(crate) fn open_in_browser(url: &str) {
    if cfg!(target_os = "macos") {
        drop(Command::new("open").arg(url).status());
    } else {
        drop(Command::new("xdg-open").arg(url).status());
    }
}
