/// Captured output from one narrow operating-system trust command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostCommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

impl HostCommandOutput {
    pub(crate) fn success(stdout: impl Into<String>) -> Self {
        Self {
            success: true,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    pub(crate) fn from_process(success: bool, stdout: Vec<u8>, stderr: Vec<u8>) -> Self {
        Self {
            success,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        }
    }

    pub(crate) const fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn stdout(&self) -> &str {
        &self.stdout
    }

    pub(crate) fn stderr(&self) -> &str {
        &self.stderr
    }
}
