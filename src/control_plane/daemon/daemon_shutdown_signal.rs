/// Read-only shutdown state observed at daemon iteration boundaries.
pub(crate) trait DaemonShutdownSignal {
    fn is_requested(&self) -> bool;
}
