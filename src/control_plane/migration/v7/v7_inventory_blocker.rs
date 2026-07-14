/// Exact legacy state that prevents unattended migration.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum V7InventoryBlocker {
    MissingContainer {
        service_id: String,
        container_name: String,
    },
    MissingImageIdentity {
        service_id: String,
        container_name: String,
    },
    HostBindVolume {
        service_id: String,
        source: String,
        target: String,
    },
    AnonymousVolume {
        service_id: String,
        target: String,
    },
    UnsupportedVolume {
        service_id: String,
        mount: String,
    },
    VolumeMismatch {
        service_id: String,
        target: String,
        expected_source: String,
        observed_source: Option<String>,
    },
    UnexpectedVolume {
        service_id: String,
        source: String,
        target: String,
    },
    SwarmTargets {
        count: usize,
    },
}
