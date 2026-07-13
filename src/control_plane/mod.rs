mod application;
mod configuration;
mod daemon;
mod desired_state;
mod dns_label;
mod engine;
mod gateway;
mod identity_error;
mod project_identity;
mod registry_conflict;
mod route_claim;
mod route_identity;
mod service_identity;
mod shared_infrastructure;
mod state;
mod validate_route_claims;
mod validated_route_registry;

pub(crate) use desired_state::{DesiredProject, DesiredProjectError, resolve_desired_project};
use dns_label::DnsLabel;
pub(crate) use identity_error::IdentityError;
pub(crate) use project_identity::ProjectIdentity;
pub(crate) use registry_conflict::{RegistryConflict, RegistryConflicts};
pub(crate) use route_claim::RouteClaim;
pub(crate) use route_identity::RouteIdentity;
pub(crate) use service_identity::ServiceIdentity;
pub(crate) use shared_infrastructure::{
    CompatibilityFingerprint, CompatibilityFingerprintOptions, IsolationCapability, PersistenceMode,
};
pub(crate) use validate_route_claims::validate_route_claims;
pub(crate) use validated_route_registry::ValidatedRouteRegistry;

#[cfg(test)]
mod tests;
