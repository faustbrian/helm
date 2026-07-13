/// Side-effect-free outcome of evaluating a managed resource for deletion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum DeletionDecision {
    KeepActive,
    StopAndRetain,
    DeleteDisposable,
    AwaitVerifiedBackup,
    DeleteAuthorized,
}
