/// Structured host command allowed at a narrow OS integration boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostCommand {
    program: String,
    arguments: Vec<String>,
}

impl HostCommand {
    pub(crate) fn new<I, A>(program: impl Into<String>, arguments: I) -> Self
    where
        I: IntoIterator<Item = A>,
        A: Into<String>,
    {
        Self {
            program: program.into(),
            arguments: arguments.into_iter().map(Into::into).collect(),
        }
    }

    pub(crate) fn program(&self) -> &str {
        &self.program
    }

    pub(crate) fn arguments(&self) -> &[String] {
        &self.arguments
    }
}
