#[cfg(test)]
mod tests;

pub(crate) use classify_logical_resource_error::classify_logical_resource_error;
pub(crate) use compatibility_fingerprint::CompatibilityFingerprint;
pub(crate) use compatibility_fingerprint_options::CompatibilityFingerprintOptions;
pub(crate) use compatibility_profile::CompatibilityProfile;
pub(crate) use credential_entropy::CredentialEntropy;
pub(crate) use credential_generation_error::CredentialGenerationError;
pub(crate) use credential_secret::CredentialSecret;
pub(crate) use generate_credential_secret::generate_credential_secret;
pub(crate) use gotenberg::{
    GotenbergPreparationOptions, PreparedGotenbergSharedInstance,
    prepare_gotenberg_shared_instances, reconcile_prepared_gotenberg_instance,
};
pub(crate) use isolation_capability::IsolationCapability;
pub(crate) use logical_resource_drift::LogicalResourceDrift;
pub(crate) use mailpit::{
    MailpitPreparationOptions, PreparedMailpitSharedInstance, prepare_mailpit_shared_instances,
    reconcile_prepared_mailpit_instance,
};
pub(crate) use managed_secret_store_error::ManagedSecretStoreError;
#[cfg(test)]
pub(crate) use mongodb::wait_for_mongodb_readiness;
pub(crate) use mongodb::{
    MongoDbAccessRevocationOptions, MongoDbLogicalResourcePlan, MongoDbMigrationPreparationOptions,
    MongoDbPreparationOptions, MongoDbSharedInstancePlan, MongoDbSharedInstancePlanOptions,
    PreparedMongoDbSharedInstance, plan_mongodb_project_resources,
    prepare_mongodb_shared_instances, provision_mongodb_logical_resource,
    reconcile_mongodb_migration_target, reconcile_prepared_mongodb_instance,
    revoke_mongodb_project_access,
};
pub(crate) use mysql::{
    MySqlAccessRevocationOptions, MySqlFlavor, MySqlLogicalResourcePlan,
    MySqlMigrationPreparationOptions, MySqlPreparationOptions, MySqlSharedInstancePlan,
    MySqlSharedInstancePlanOptions, PreparedMySqlSharedInstance, plan_mysql_project_resources,
    prepare_mysql_shared_instances, provision_mysql_logical_resource,
    reconcile_mysql_migration_target, reconcile_prepared_mysql_instance,
    revoke_mysql_project_access,
};
pub(crate) use object_store::{
    MinioAccessRevocationOptions, ObjectStorePreparationOptions, PreparedObjectStoreSharedInstance,
    prepare_object_store_shared_instances, reconcile_prepared_object_store_instance,
    revoke_minio_project_access,
};
pub(crate) use orphaned_shared_access_options::OrphanedSharedAccessOptions;
pub(crate) use os_credential_entropy::OsCredentialEntropy;
pub(crate) use persistence_mode::PersistenceMode;
pub(crate) use plan_shared_instances::plan_shared_instances;
pub(crate) use postgres::{
    PostgresAccessRevocationOptions, PostgresLogicalResourcePlan,
    PostgresMigrationPreparationOptions, PostgresMigrationTargetReconcileResult,
    PostgresPreparationOptions, PostgresProjectResources, PostgresSharedInstancePlan,
    PostgresSharedInstancePlanOptions, PreparedPostgresSharedInstance,
    plan_postgres_project_resources, prepare_postgres_shared_instances,
    provision_postgres_logical_resource, reconcile_postgres_migration_target,
    reconcile_prepared_postgres_instance, revoke_postgres_project_access,
};
pub(crate) use prepare_shared_instances::prepare_shared_instances;
pub(crate) use prepared_shared_instance::PreparedSharedInstance;
pub(crate) use provisioning_job_options::ProvisioningJobOptions;
pub(crate) use provisioning_jobs_run_options::ProvisioningJobsRunOptions;
pub(crate) use rabbitmq::{
    PreparedRabbitMqSharedInstance, RabbitMqPreparationOptions, RabbitMqProjectDefinition,
    RabbitMqSharedInstancePlan, prepare_rabbitmq_shared_instances,
    reconcile_prepared_rabbitmq_instance, revoke_rabbitmq_project_access,
    wait_for_rabbitmq_readiness,
};
pub(crate) use reconcile_prepared_shared_instance::reconcile_prepared_shared_instance;
pub(crate) use reconcile_shared_service::reconcile_shared_service;
pub(crate) use reconcile_shared_volume::reconcile_shared_volume;
pub(crate) use redis::{
    PreparedRedisSharedInstance, RedisAccessRevocationOptions, RedisFlavor,
    RedisPreparationOptions, prepare_redis_shared_instances, reconcile_prepared_redis_instance,
    revoke_redis_project_access,
};
pub(crate) use resolve_execution_shared_instances::resolve_execution_shared_instances;
#[cfg(test)]
pub(crate) use revoke_orphaned_shared_access::revoke_orphaned_shared_access;
pub(crate) use revoke_orphaned_shared_access::revoke_orphaned_shared_access_from_observed;
#[cfg(test)]
pub(crate) use run_provisioning_job::run_provisioning_job;
pub(crate) use run_provisioning_job::run_provisioning_job_from_observed;
pub(crate) use run_provisioning_jobs_from_observed::run_provisioning_jobs_from_observed;
pub(crate) use run_shared_service_readiness_probe::run_shared_service_readiness_probe;
pub(crate) use shared_container_name::shared_container_name;
pub(crate) use shared_demand_plan_error::SharedDemandPlanError;
pub(crate) use shared_identity_hex::shared_identity_hex;
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
    PreparedSqlServerSharedInstance, SqlServerAccessRevocationOptions,
    SqlServerLogicalResourcePlan, SqlServerMigrationPreparationOptions,
    SqlServerPreparationOptions, SqlServerSharedInstancePlan, SqlServerSharedInstancePlanOptions,
    plan_sql_server_project_resources, prepare_sql_server_shared_instances,
    provision_sql_server_logical_resource, reconcile_prepared_sql_server_instance,
    reconcile_sql_server_migration_target, revoke_sql_server_project_access,
};
#[cfg(test)]
pub(crate) use stop_unreferenced_shared_services::stop_unreferenced_shared_services;
pub(crate) use stop_unreferenced_shared_services::stop_unreferenced_shared_services_from_observed;
pub(crate) use store_credential_secret::store_credential_secret;
pub(crate) use unreferenced_shared_service_options::UnreferencedSharedServiceOptions;

#[cfg(test)]
pub(crate) use gotenberg::{
    GotenbergSharedInstancePlan, GotenbergSharedInstancePlanOptions,
    plan_gotenberg_project_resources,
};
#[cfg(test)]
pub(crate) use mailpit::{
    MailpitAuthenticationSnapshot, MailpitProjectDefinition, MailpitSharedInstancePlan,
    MailpitSharedInstancePlanOptions, plan_mailpit_project_resources,
    reconcile_mailpit_authentication, store_mailpit_authentication,
};
#[cfg(test)]
pub(crate) use mongodb::{
    MongoDbMigrationInstancePlanOptions, prepare_mongodb_migration_target,
    reconcile_mongodb_project_resources,
};
#[cfg(test)]
pub(crate) use mysql::{
    MySqlMigrationInstancePlanOptions, prepare_mysql_migration_target,
    reconcile_mysql_project_resources,
};
#[cfg(test)]
pub(crate) use object_store::{
    ObjectStoreFlavor, ObjectStoreProjectResources, ObjectStoreSharedInstancePlan,
    ObjectStoreSharedInstancePlanOptions, plan_object_store_project_resources,
    provision_object_store_project_resources, reconcile_object_store_project_resources,
};
#[cfg(test)]
pub(crate) use postgres::{
    PostgresMigrationInstancePlanOptions, prepare_postgres_migration_target,
    reconcile_postgres_project_resources,
};
#[cfg(test)]
pub(crate) use rabbitmq::{
    RabbitMqDefinitions, RabbitMqPasswordHash, RabbitMqSharedInstancePlanOptions,
    plan_rabbitmq_project_resources, reconcile_rabbitmq_definitions, reload_rabbitmq_definitions,
    store_rabbitmq_definitions,
};
#[cfg(test)]
pub(crate) use redis::{
    RedisAclProject, RedisAclSnapshot, RedisSharedInstancePlan, RedisSharedInstancePlanOptions,
    plan_redis_project_resources, reconcile_redis_acl_snapshot, reload_redis_acl,
    store_redis_acl_snapshot,
};
#[cfg(test)]
pub(crate) use sql_server::{
    SqlServerMigrationInstancePlanOptions, prepare_sql_server_migration_target,
    reconcile_sql_server_project_resources,
};

mod classify_logical_resource_error;
mod compatibility_fingerprint;
mod compatibility_fingerprint_error;
mod compatibility_fingerprint_options;
mod compatibility_profile;
mod credential_entropy;
mod credential_generation_error;
mod credential_secret;
mod ensure_prepared_shared_image;
mod generate_credential_secret;
mod gotenberg;
mod isolation_capability;
mod logical_resource_drift;
mod logical_service_consumer;
mod mailpit;
mod managed_secret_store_error;
mod mongodb;
mod mysql;
mod object_store;
mod orphaned_shared_access_options;
mod os_credential_entropy;
mod persistence_mode;
mod plan_shared_instances;
mod postgres;
mod prepare_shared_instances;
mod prepared_shared_instance;
mod provisioning_job_options;
mod provisioning_jobs_run_options;
mod rabbitmq;
mod reconcile_prepared_shared_instance;
mod reconcile_shared_service;
mod reconcile_shared_volume;
mod redis;
mod resolve_execution_shared_instances;
mod revoke_orphaned_shared_access;
mod run_provisioning_job;
mod run_provisioning_jobs_from_observed;
mod run_shared_service_readiness_probe;
mod shared_container_name;
mod shared_demand_plan_error;
mod shared_identity_hex;
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
mod stop_unreferenced_shared_services;
mod store_credential_secret;
mod unreferenced_shared_service_options;
