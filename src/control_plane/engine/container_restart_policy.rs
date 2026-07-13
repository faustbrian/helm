/// Supported restart behavior for Stackctl-owned containers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ContainerRestartPolicy {
    UnlessStopped,
}
