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
pub(crate) use managed_secret_store_error::ManagedSecretStoreError;
pub(crate) use mongodb::{
    MongoDbLogicalResourcePlan, MongoDbPlanError, MongoDbProjectResources,
    MongoDbSharedInstancePlan, MongoDbSharedInstancePlanOptions, plan_mongodb_project_resources,
    provision_mongodb_logical_resource,
};
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
    provision_postgres_logical_resource, reconcile_postgres_project_resources,
};
pub(crate) use rabbitmq::{
    RabbitMqDefinitions, RabbitMqPasswordHash, RabbitMqPlanError, RabbitMqProjectDefinition,
    RabbitMqProjectResources, RabbitMqSharedInstancePlan, RabbitMqSharedInstancePlanOptions,
    StoredRabbitMqPaths, plan_rabbitmq_project_resources, reload_rabbitmq_definitions,
    revoke_rabbitmq_project_access, store_rabbitmq_definitions,
};
pub(crate) use reconcile_shared_service::reconcile_shared_service;
pub(crate) use reconcile_shared_volume::reconcile_shared_volume;
pub(crate) use redis::{
    RedisAclProject, RedisAclSnapshot, RedisFlavor, RedisPlanError, RedisProjectResources,
    RedisSharedInstancePlan, RedisSharedInstancePlanOptions, StoredRedisAclPaths,
    plan_redis_project_resources, reload_redis_acl, store_redis_acl_snapshot,
};
pub(crate) use shared_infrastructure_reconcile_error::SharedInfrastructureReconcileError;
pub(crate) use shared_instance_plan::SharedInstancePlan;
pub(crate) use shared_service_reconcile_action::SharedServiceReconcileAction;
pub(crate) use shared_service_reconcile_options::SharedServiceReconcileOptions;
pub(crate) use shared_service_reconcile_result::SharedServiceReconcileResult;
pub(crate) use shared_service_request::SharedServiceRequest;
pub(crate) use shared_volume_reconcile_action::SharedVolumeReconcileAction;
pub(crate) use shared_volume_reconcile_options::SharedVolumeReconcileOptions;
pub(crate) use shared_volume_reconcile_result::SharedVolumeReconcileResult;
pub(crate) use store_credential_secret::store_credential_secret;

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
mod managed_secret_store_error;
mod mongodb;
mod mysql;
mod os_credential_entropy;
mod persistence_mode;
mod plan_shared_instances;
mod postgres;
mod rabbitmq;
mod reconcile_shared_service;
mod reconcile_shared_volume;
mod redis;
mod shared_infrastructure_reconcile_error;
mod shared_instance_plan;
mod shared_service_reconcile_action;
mod shared_service_reconcile_options;
mod shared_service_reconcile_result;
mod shared_service_request;
mod shared_volume_reconcile_action;
mod shared_volume_reconcile_options;
mod shared_volume_reconcile_result;
mod store_credential_secret;
