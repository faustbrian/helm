/// Stable terminal or operator-gated result from migration reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MigrationExecutionResult {
    AwaitingConfirmation,
    Confirmed,
    RolledBack,
}
