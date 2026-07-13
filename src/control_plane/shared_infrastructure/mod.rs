#[cfg(test)]
mod tests;

pub(crate) use compatibility_fingerprint::CompatibilityFingerprint;
pub(crate) use compatibility_fingerprint_options::CompatibilityFingerprintOptions;
pub(crate) use compatibility_profile::CompatibilityProfile;
pub(crate) use credential_entropy::CredentialEntropy;
pub(crate) use credential_generation_error::CredentialGenerationError;
pub(crate) use credential_secret::CredentialSecret;
pub(crate) use generate_credential_secret::generate_credential_secret;
pub(crate) use gotenberg::{
    GotenbergPlanError, GotenbergProjectResources, GotenbergSharedInstancePlan,
    GotenbergSharedInstancePlanOptions, plan_gotenberg_project_resources,
};
pub(crate) use isolation_capability::IsolationCapability;
pub(crate) use mailpit::{
    MailpitAuthenticationSnapshot, MailpitPlanError, MailpitProjectDefinition,
    MailpitProjectResources, MailpitSharedInstancePlan, MailpitSharedInstancePlanOptions,
    StoredMailpitAuthenticationPaths, plan_mailpit_project_resources,
    reconcile_mailpit_authentication, store_mailpit_authentication,
};
pub(crate) use managed_secret_store_error::ManagedSecretStoreError;
pub(crate) use mongodb::{
    MongoDbLogicalResourcePlan, MongoDbPlanError, MongoDbProjectResources,
    MongoDbSharedInstancePlan, MongoDbSharedInstancePlanOptions, plan_mongodb_project_resources,
    provision_mongodb_logical_resource, reconcile_mongodb_project_resources,
};
pub(crate) use mysql::{
    MySqlFlavor, MySqlLogicalResourcePlan, MySqlPlanError, MySqlPreparationError,
    MySqlPreparationOptions, MySqlProjectResources, MySqlSharedInstancePlan,
    MySqlSharedInstancePlanOptions, PreparedMySqlSharedInstance, plan_mysql_project_resources,
    prepare_mysql_shared_instances, provision_mysql_logical_resource,
    reconcile_mysql_project_resources, reconcile_prepared_mysql_instance,
};
pub(crate) use object_store::{
    ObjectStoreFlavor, ObjectStorePlanError, ObjectStoreProjectDefinition,
    ObjectStoreProjectResources, ObjectStoreSharedInstancePlan,
    ObjectStoreSharedInstancePlanOptions, plan_object_store_project_resources,
    provision_object_store_project_resources, reconcile_object_store_project_resources,
    store_object_store_policy,
};
pub(crate) use os_credential_entropy::OsCredentialEntropy;
pub(crate) use persistence_mode::PersistenceMode;
pub(crate) use plan_shared_instances::plan_shared_instances;
pub(crate) use postgres::{
    PostgresLogicalResourcePlan, PostgresPlanError, PostgresPreparationError,
    PostgresPreparationOptions, PostgresProjectResources, PostgresSharedInstancePlan,
    PostgresSharedInstancePlanOptions, PreparedPostgresSharedInstance,
    plan_postgres_project_resources, prepare_postgres_shared_instances,
    provision_postgres_logical_resource, reconcile_postgres_project_resources,
    reconcile_prepared_postgres_instance,
};
pub(crate) use prepare_shared_instances::prepare_shared_instances;
pub(crate) use prepared_shared_instance::PreparedSharedInstance;
pub(crate) use provisioning_job_options::ProvisioningJobOptions;
pub(crate) use rabbitmq::{
    RabbitMqDefinitions, RabbitMqPasswordHash, RabbitMqPlanError, RabbitMqProjectDefinition,
    RabbitMqProjectResources, RabbitMqSharedInstancePlan, RabbitMqSharedInstancePlanOptions,
    StoredRabbitMqPaths, plan_rabbitmq_project_resources, reconcile_rabbitmq_definitions,
    reload_rabbitmq_definitions, revoke_rabbitmq_project_access, store_rabbitmq_definitions,
};
pub(crate) use reconcile_prepared_shared_instance::reconcile_prepared_shared_instance;
pub(crate) use reconcile_shared_service::reconcile_shared_service;
pub(crate) use reconcile_shared_volume::reconcile_shared_volume;
pub(crate) use redis::{
    RedisAclProject, RedisAclSnapshot, RedisFlavor, RedisPlanError, RedisProjectResources,
    RedisSharedInstancePlan, RedisSharedInstancePlanOptions, StoredRedisAclPaths,
    plan_redis_project_resources, reconcile_redis_acl_snapshot, reload_redis_acl,
    store_redis_acl_snapshot,
};
pub(crate) use resolve_execution_shared_instances::resolve_execution_shared_instances;
pub(crate) use run_provisioning_job::run_provisioning_job;
pub(crate) use shared_demand_plan_error::SharedDemandPlanError;
pub(crate) use shared_infrastructure_reconcile_error::SharedInfrastructureReconcileError;
pub(crate) use shared_instance_plan::SharedInstancePlan;
pub(crate) use shared_instance_reconcile_result::SharedInstanceReconcileResult;
pub(crate) use shared_preparation_error::SharedPreparationError;
pub(crate) use shared_preparation_options::SharedPreparationOptions;
pub(crate) use shared_service_reconcile_action::SharedServiceReconcileAction;
pub(crate) use shared_service_reconcile_options::SharedServiceReconcileOptions;
pub(crate) use shared_service_reconcile_result::SharedServiceReconcileResult;
pub(crate) use shared_service_request::SharedServiceRequest;
pub(crate) use shared_volume_reconcile_action::SharedVolumeReconcileAction;
pub(crate) use shared_volume_reconcile_options::SharedVolumeReconcileOptions;
pub(crate) use shared_volume_reconcile_result::SharedVolumeReconcileResult;
pub(crate) use sql_server::{
    SqlServerLogicalResourcePlan, SqlServerPlanError, SqlServerProjectResources,
    SqlServerSharedInstancePlan, SqlServerSharedInstancePlanOptions,
    plan_sql_server_project_resources, provision_sql_server_logical_resource,
    reconcile_sql_server_project_resources,
};
pub(crate) use store_credential_secret::store_credential_secret;

mod compatibility_fingerprint;
mod compatibility_fingerprint_error;
mod compatibility_fingerprint_options;
mod compatibility_profile;
mod credential_entropy;
mod credential_generation_error;
mod credential_secret;
mod generate_credential_secret;
mod gotenberg;
mod isolation_capability;
mod logical_service_consumer;
mod mailpit;
mod managed_secret_store_error;
mod mongodb;
mod mysql;
mod object_store;
mod os_credential_entropy;
mod persistence_mode;
mod plan_shared_instances;
mod postgres;
mod prepare_shared_instances;
mod prepared_shared_instance;
mod provisioning_job_options;
mod rabbitmq;
mod reconcile_prepared_shared_instance;
mod reconcile_shared_service;
mod reconcile_shared_volume;
mod redis;
mod resolve_execution_shared_instances;
mod run_provisioning_job;
mod shared_demand_plan_error;
mod shared_infrastructure_reconcile_error;
mod shared_instance_plan;
mod shared_instance_reconcile_result;
mod shared_preparation_error;
mod shared_preparation_options;
mod shared_service_reconcile_action;
mod shared_service_reconcile_options;
mod shared_service_reconcile_result;
mod shared_service_request;
mod shared_volume_reconcile_action;
mod shared_volume_reconcile_options;
mod shared_volume_reconcile_result;
mod sql_server;
mod store_credential_secret;
