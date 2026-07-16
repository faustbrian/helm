/// Captured output from one narrow operating-system trust command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostCommandOutput {
    success: bool,
    #[cfg(any(target_os = "macos", test))]
    stdout: String,
    stderr: String,
}

impl HostCommandOutput {
    #[cfg(test)]
    pub(crate) fn success(stdout: impl Into<String>) -> Self {
        Self {
            success: true,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn failure(stderr: impl Into<String>) -> Self {
        Self {
            success: false,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    pub(crate) fn from_process(success: bool, stdout: Vec<u8>, stderr: Vec<u8>) -> Self {
        #[cfg(all(target_os = "linux", not(test)))]
        drop(stdout);

        Self {
            success,
            #[cfg(any(target_os = "macos", test))]
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        }
    }

    pub(crate) const fn succeeded(&self) -> bool {
        self.success
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn stdout(&self) -> &str {
        &self.stdout
    }

    pub(crate) fn stderr(&self) -> &str {
        &self.stderr
    }
}
