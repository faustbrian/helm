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
    GotenbergPlanError, GotenbergPreparationError, GotenbergPreparationOptions,
    GotenbergProjectResources, GotenbergSharedInstancePlan, GotenbergSharedInstancePlanOptions,
    PreparedGotenbergSharedInstance, plan_gotenberg_project_resources,
    prepare_gotenberg_shared_instances, reconcile_prepared_gotenberg_instance,
};
pub(crate) use isolation_capability::IsolationCapability;
pub(crate) use logical_resource_drift::LogicalResourceDrift;
pub(crate) use mailpit::{
    MailpitAuthenticationSnapshot, MailpitPlanError, MailpitPreparationError,
    MailpitPreparationOptions, MailpitProjectDefinition, MailpitProjectResources,
    MailpitSharedInstancePlan, MailpitSharedInstancePlanOptions, PreparedMailpitSharedInstance,
    StoredMailpitAuthenticationPaths, plan_mailpit_project_resources,
    prepare_mailpit_shared_instances, reconcile_mailpit_authentication,
    reconcile_prepared_mailpit_instance, store_mailpit_authentication,
};
pub(crate) use managed_secret_store_error::ManagedSecretStoreError;
#[cfg(test)]
pub(crate) use mongodb::wait_for_mongodb_readiness;
pub(crate) use mongodb::{
    MongoDbAccessRevocationOptions, MongoDbLogicalResourcePlan,
    MongoDbMigrationInstancePlanOptions, MongoDbMigrationPreparationOptions,
    MongoDbMigrationTargetReconcileResult, MongoDbPlanError, MongoDbPreparationError,
    MongoDbPreparationOptions, MongoDbProjectResources, MongoDbSharedInstancePlan,
    MongoDbSharedInstancePlanOptions, PreparedMongoDbSharedInstance,
    plan_mongodb_project_resources, prepare_mongodb_migration_target,
    prepare_mongodb_shared_instances, provision_mongodb_logical_resource,
    reconcile_mongodb_migration_target, reconcile_mongodb_project_resources,
    reconcile_prepared_mongodb_instance, revoke_mongodb_project_access,
};
pub(crate) use mysql::{
    MySqlAccessRevocationOptions, MySqlFlavor, MySqlLogicalResourcePlan,
    MySqlMigrationInstancePlanOptions, MySqlMigrationPreparationOptions,
    MySqlMigrationTargetReconcileResult, MySqlPlanError, MySqlPreparationError,
    MySqlPreparationOptions, MySqlProjectResources, MySqlSharedInstancePlan,
    MySqlSharedInstancePlanOptions, PreparedMySqlSharedInstance, plan_mysql_project_resources,
    prepare_mysql_migration_target, prepare_mysql_shared_instances,
    provision_mysql_logical_resource, reconcile_mysql_migration_target,
    reconcile_mysql_project_resources, reconcile_prepared_mysql_instance,
    revoke_mysql_project_access,
};
pub(crate) use object_store::{
    MinioAccessRevocationOptions, ObjectStoreFlavor, ObjectStorePlanError,
    ObjectStorePreparationError, ObjectStorePreparationOptions, ObjectStoreProjectDefinition,
    ObjectStoreProjectResources, ObjectStoreSharedInstancePlan,
    ObjectStoreSharedInstancePlanOptions, PreparedObjectStoreSharedInstance,
    plan_object_store_project_resources, prepare_object_store_shared_instances,
    provision_object_store_project_resources, reconcile_object_store_project_resources,
    reconcile_prepared_object_store_instance, revoke_minio_project_access,
    store_object_store_policy,
};
pub(crate) use orphaned_shared_access_options::OrphanedSharedAccessOptions;
pub(crate) use os_credential_entropy::OsCredentialEntropy;
pub(crate) use persistence_mode::PersistenceMode;
pub(crate) use plan_shared_instances::plan_shared_instances;
pub(crate) use postgres::{
    PostgresAccessRevocationOptions, PostgresLogicalResourcePlan,
    PostgresMigrationInstancePlanOptions, PostgresMigrationPreparationOptions,
    PostgresMigrationTargetReconcileError, PostgresMigrationTargetReconcileResult,
    PostgresPlanError, PostgresPreparationError, PostgresPreparationOptions,
    PostgresProjectResources, PostgresSharedInstancePlan, PostgresSharedInstancePlanOptions,
    PreparedPostgresSharedInstance, plan_postgres_project_resources,
    prepare_postgres_migration_target, prepare_postgres_shared_instances,
    provision_postgres_logical_resource, reconcile_postgres_migration_target,
    reconcile_postgres_project_resources, reconcile_prepared_postgres_instance,
    revoke_postgres_project_access,
};
pub(crate) use prepare_shared_instances::prepare_shared_instances;
pub(crate) use prepared_shared_instance::PreparedSharedInstance;
pub(crate) use provisioning_job_options::ProvisioningJobOptions;
pub(crate) use provisioning_jobs_run_options::ProvisioningJobsRunOptions;
pub(crate) use rabbitmq::{
    PreparedRabbitMqSharedInstance, RabbitMqDefinitions, RabbitMqPasswordHash, RabbitMqPlanError,
    RabbitMqPreparationError, RabbitMqPreparationOptions, RabbitMqProjectDefinition,
    RabbitMqProjectResources, RabbitMqSharedInstancePlan, RabbitMqSharedInstancePlanOptions,
    StoredRabbitMqPaths, plan_rabbitmq_project_resources, prepare_rabbitmq_shared_instances,
    reconcile_prepared_rabbitmq_instance, reconcile_rabbitmq_definitions,
    reload_rabbitmq_definitions, revoke_rabbitmq_project_access, store_rabbitmq_definitions,
    wait_for_rabbitmq_readiness,
};
pub(crate) use reconcile_prepared_shared_instance::reconcile_prepared_shared_instance;
pub(crate) use reconcile_shared_service::reconcile_shared_service;
pub(crate) use reconcile_shared_volume::reconcile_shared_volume;
pub(crate) use redis::{
    PreparedRedisSharedInstance, RedisAccessRevocationOptions, RedisAclProject, RedisAclSnapshot,
    RedisFlavor, RedisPlanError, RedisPreparationError, RedisPreparationOptions,
    RedisProjectResources, RedisSharedInstancePlan, RedisSharedInstancePlanOptions,
    StoredRedisAclPaths, plan_redis_project_resources, prepare_redis_shared_instances,
    reconcile_prepared_redis_instance, reconcile_redis_acl_snapshot, reload_redis_acl,
    revoke_redis_project_access, store_redis_acl_snapshot,
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
    PreparedSqlServerSharedInstance, SqlServerAccessRevocationOptions,
    SqlServerLogicalResourcePlan, SqlServerMigrationInstancePlanOptions,
    SqlServerMigrationPreparationOptions, SqlServerMigrationTargetReconcileResult,
    SqlServerPlanError, SqlServerPreparationError, SqlServerPreparationOptions,
    SqlServerProjectResources, SqlServerSharedInstancePlan, SqlServerSharedInstancePlanOptions,
    plan_sql_server_project_resources, prepare_sql_server_migration_target,
    prepare_sql_server_shared_instances, provision_sql_server_logical_resource,
    reconcile_prepared_sql_server_instance, reconcile_sql_server_migration_target,
    reconcile_sql_server_project_resources, revoke_sql_server_project_access,
};
#[cfg(test)]
pub(crate) use stop_unreferenced_shared_services::stop_unreferenced_shared_services;
pub(crate) use stop_unreferenced_shared_services::stop_unreferenced_shared_services_from_observed;
pub(crate) use store_credential_secret::store_credential_secret;
pub(crate) use unreferenced_shared_service_options::UnreferencedSharedServiceOptions;

mod classify_logical_resource_error;
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
mod stop_unreferenced_shared_services;
mod store_credential_secret;
mod unreferenced_shared_service_options;
