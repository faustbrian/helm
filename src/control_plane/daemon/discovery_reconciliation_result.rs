use super::ProjectDiscoveryReport;
use crate::control_plane::application::DesiredRegistry;

/// One complete scan and whether it was safe to publish as authoritative.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiscoveryReconciliationResult {
    report: ProjectDiscoveryReport,
    registry: Option<DesiredRegistry>,
}

impl DiscoveryReconciliationResult {
    pub(super) fn applied(report: ProjectDiscoveryReport, registry: DesiredRegistry) -> Self {
        Self {
            report,
            registry: Some(registry),
        }
    }

    pub(super) fn blocked(report: ProjectDiscoveryReport) -> Self {
        Self {
            report,
            registry: None,
        }
    }

    pub(crate) const fn report(&self) -> &ProjectDiscoveryReport {
        &self.report
    }

    pub(crate) const fn was_applied(&self) -> bool {
        self.registry.is_some()
    }

    pub(crate) const fn registry(&self) -> Option<&DesiredRegistry> {
        self.registry.as_ref()
    }
}
