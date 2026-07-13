#[cfg(test)]
mod tests;

pub(crate) use compatibility_fingerprint::CompatibilityFingerprint;
pub(crate) use compatibility_fingerprint_options::CompatibilityFingerprintOptions;
pub(crate) use compatibility_profile::CompatibilityProfile;
pub(crate) use credential_entropy::CredentialEntropy;
pub(crate) use credential_generation_error::CredentialGenerationError;
pub(crate) use credential_secret::CredentialSecret;
pub(crate) use generate_credential_secret::generate_credential_secret;
pub(crate) use isolation_capability::IsolationCapability;
pub(crate) use mysql::{
    MySqlFlavor, MySqlLogicalResourcePlan, MySqlPlanError, MySqlProjectResources,
    MySqlSharedInstancePlan, MySqlSharedInstancePlanOptions, plan_mysql_project_resources,
    provision_mysql_logical_resource,
};
pub(crate) use os_credential_entropy::OsCredentialEntropy;
pub(crate) use persistence_mode::PersistenceMode;
pub(crate) use plan_shared_instances::plan_shared_instances;
pub(crate) use postgres::{
    PostgresLogicalResourcePlan, PostgresPlanError, PostgresProjectResources,
    PostgresSharedInstancePlan, PostgresSharedInstancePlanOptions, plan_postgres_project_resources,
    provision_postgres_logical_resource,
};
pub(crate) use shared_instance_plan::SharedInstancePlan;
pub(crate) use shared_service_request::SharedServiceRequest;

mod compatibility_fingerprint;
mod compatibility_fingerprint_error;
mod compatibility_fingerprint_options;
mod compatibility_profile;
mod credential_entropy;
mod credential_generation_error;
mod credential_secret;
mod generate_credential_secret;
mod isolation_capability;
mod logical_service_consumer;
mod mysql;
mod os_credential_entropy;
mod persistence_mode;
mod plan_shared_instances;
mod postgres;
mod shared_instance_plan;
mod shared_service_request;
