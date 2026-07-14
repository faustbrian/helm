/// Secret-free service fields needed to choose one migration adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct V7MigrationServiceSource<'source> {
    pub(crate) service_id: &'source str,
    pub(crate) driver: &'source str,
    pub(crate) named_volumes: &'source [&'source str],
}

/// Exact accepted route fields needed to validate route ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct V7MigrationRouteSource<'source> {
    pub(crate) service_id: &'source str,
    pub(crate) domain: &'source str,
    pub(crate) scheme: &'source str,
    pub(crate) host_port: u16,
}

/// Complete accepted evidence projection used for adapter selection.
#[derive(Clone, Copy, Debug)]
pub(crate) struct V7MigrationAdapterSelectionOptions<'source> {
    pub(crate) evidence_revision: &'source str,
    pub(crate) services: &'source [V7MigrationServiceSource<'source>],
    pub(crate) routes: &'source [V7MigrationRouteSource<'source>],
    pub(crate) requires_legacy_ca_capture: bool,
    pub(crate) captured_ca_certificates: usize,
    pub(crate) generated_environment_present: bool,
    pub(crate) protected_generated_environment: bool,
}
