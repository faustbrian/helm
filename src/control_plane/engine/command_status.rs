/// Current state of an Engine exec process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum CommandStatus {
    Running,
    Exited(i64),
}
