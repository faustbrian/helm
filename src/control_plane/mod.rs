mod dns_label;
mod identity_error;
mod project_identity;
mod registry_conflict;
mod route_claim;
mod route_identity;
mod service_identity;
mod validate_route_claims;
mod validated_route_registry;

use dns_label::DnsLabel;
pub(crate) use identity_error::IdentityError;
pub(crate) use project_identity::ProjectIdentity;
pub(crate) use registry_conflict::{RegistryConflict, RegistryConflicts};
pub(crate) use route_claim::RouteClaim;
pub(crate) use route_identity::RouteIdentity;
pub(crate) use service_identity::ServiceIdentity;
pub(crate) use validate_route_claims::validate_route_claims;
pub(crate) use validated_route_registry::ValidatedRouteRegistry;

#[cfg(test)]
mod tests;
