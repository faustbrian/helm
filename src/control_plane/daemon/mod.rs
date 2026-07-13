mod discover_project_sources;
mod ipc;
mod project_discovery_error;
mod project_discovery_issue;
mod project_discovery_options;
mod project_discovery_report;
mod singleton_lease;
mod singleton_lease_error;

pub(crate) use singleton_lease::SingletonLease;
pub(crate) use singleton_lease_error::SingletonLeaseError;

#[cfg(test)]
mod tests;
pub(crate) use discover_project_sources::discover_project_sources;
pub(crate) use project_discovery_error::ProjectDiscoveryError;
pub(crate) use project_discovery_issue::ProjectDiscoveryIssue;
pub(crate) use project_discovery_options::ProjectDiscoveryOptions;
pub(crate) use project_discovery_report::ProjectDiscoveryReport;
