mod dns_label;
mod identity_error;
mod project_identity;
mod route_identity;
mod service_identity;

use dns_label::DnsLabel;
pub(crate) use identity_error::IdentityError;
pub(crate) use project_identity::ProjectIdentity;
pub(crate) use route_identity::RouteIdentity;
pub(crate) use service_identity::ServiceIdentity;

#[cfg(test)]
mod tests;
