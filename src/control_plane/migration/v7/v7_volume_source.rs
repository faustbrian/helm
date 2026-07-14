/// Exact source class of one v7 container volume mount.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum V7VolumeSource {
    Named(String),
    HostBind(String),
    Anonymous,
    Unsupported,
}
