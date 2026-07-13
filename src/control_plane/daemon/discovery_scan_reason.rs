/// Why the daemon must perform one complete watched-root scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiscoveryScanReason {
    Initial,
    FilesystemEvents,
    Periodic,
}
