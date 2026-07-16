//! cli support open in browser module.
//!
//! Contains cli support open in browser logic used by Stackctl command workflows.

use std::process::Command;

use anyhow::{Context, Result, bail};

#[cfg(test)]
thread_local! {
    static FAKE_OPEN_COMMAND: std::cell::RefCell<Option<String>> = const {
        std::cell::RefCell::new(None)
    };
}

fn open_command() -> String {
    #[cfg(test)]
    {
        FAKE_OPEN_COMMAND.with(|command| {
            command
                .borrow()
                .clone()
                .unwrap_or_else(default_open_command)
        })
    }
    #[cfg(not(test))]
    {
        default_open_command()
    }
}

fn default_open_command() -> String {
    if cfg!(target_os = "macos") {
        "open".to_owned()
    } else {
        "xdg-open".to_owned()
    }
}

pub(crate) fn try_open_in_browser(url: &str) -> Result<()> {
    let command = open_command();
    let status = Command::new(&command)
        .arg(url)
        .status()
        .with_context(|| format!("failed to start platform URL opener '{command}'"))?;
    if !status.success() {
        bail!("platform URL opener '{command}' exited with status {status}");
    }

    Ok(())
}

#[cfg(test)]
pub(crate) fn with_open_command<F, T>(command: &str, test: F) -> T
where
    F: FnOnce() -> T,
{
    let previous = FAKE_OPEN_COMMAND.with(|stored| {
        let mut current = stored.borrow_mut();
        let previous = current.clone();
        *current = Some(command.to_owned());
        previous
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

    FAKE_OPEN_COMMAND.with(|stored| {
        let mut current = stored.borrow_mut();
        *current = previous;
    });

    match result {
        Ok(result) => result,
        Err(err) => std::panic::resume_unwind(err),
    }
}

#[cfg(test)]
mod tests {
    use super::{try_open_in_browser, with_open_command};

    #[test]
    #[cfg(unix)]
    fn checked_open_reports_platform_command_failures() {
        let error = with_open_command("false", || {
            try_open_in_browser("https://bill-app.stackctl.localhost")
                .expect_err("platform opener failure")
        });

        assert!(error.to_string().contains("exited with status"));
    }
}
