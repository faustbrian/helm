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

impl std::fmt::Display for V7InventoryBlocker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingContainer {
                service_id,
                container_name,
            } => write!(
                formatter,
                "service '{service_id}' container '{container_name}' is absent"
            ),
            Self::MissingImageIdentity {
                service_id,
                container_name,
            } => write!(
                formatter,
                "service '{service_id}' container '{container_name}' has no observed image identity"
            ),
            Self::HostBindVolume {
                service_id,
                source,
                target,
            } => write!(
                formatter,
                "service '{service_id}' host bind '{source}:{target}' requires explicit migration"
            ),
            Self::AnonymousVolume { service_id, target } => write!(
                formatter,
                "service '{service_id}' anonymous volume at '{target}' requires explicit migration"
            ),
            Self::UnsupportedVolume { service_id, mount } => write!(
                formatter,
                "service '{service_id}' volume '{mount}' is not a supported legacy mount"
            ),
            Self::VolumeMismatch {
                service_id,
                target,
                expected_source,
                observed_source,
            } => write!(
                formatter,
                "service '{service_id}' volume at '{target}' expected source '{expected_source}' but observed '{}'",
                observed_source.as_deref().unwrap_or("<missing>")
            ),
            Self::UnexpectedVolume {
                service_id,
                source,
                target,
            } => write!(
                formatter,
                "service '{service_id}' has unexpected observed volume '{source}:{target}'"
            ),
            Self::SwarmTargets { count } => write!(
                formatter,
                "legacy project has {count} Swarm target(s) requiring explicit migration"
            ),
        }
    }
}
