#[cfg(test)]
mod tests;

mod backup_artifact_manifest;
mod backup_resource_identity;
mod backup_verification_error;
mod data_lifecycle_strategy;
mod data_lifecycle_strategy_error;
mod deletion_decision;
mod evaluate_deletion;
mod hashing_reader;
mod logical_prune_plan;
mod logical_prune_plan_options;
mod minio_logical_prune_options;
mod mongodb_logical_prune_options;
mod mysql_logical_prune_options;
mod open_stored_backup_artifact;
mod postgres_logical_prune_options;
mod prune_authorization;
mod prune_minio_logical_resource;
mod prune_mongodb_logical_resource;
mod prune_mysql_logical_resource;
mod prune_postgres_logical_resource;
mod prune_rabbitmq_logical_resource;
mod prune_sql_server_logical_resource;
mod rabbitmq_logical_prune_options;
mod resolve_data_lifecycle_strategy;
mod restore_error;
mod restore_target;
mod restore_target_error;
mod restore_verified_backup;
mod sql_server_logical_prune_options;
mod store_backup_artifact;
mod store_backup_artifact_from_async_reader;
mod stored_backup_artifact;
mod verified_backup_evidence;
mod verify_backup_artifact;
mod verify_stored_backup_artifact;

pub(crate) use backup_artifact_manifest::BackupArtifactManifest;
pub(crate) use backup_resource_identity::BackupResourceIdentity;
pub(crate) use backup_verification_error::BackupVerificationError;
pub(crate) use data_lifecycle_strategy::DataLifecycleStrategy;
pub(crate) use data_lifecycle_strategy_error::DataLifecycleStrategyError;
pub(crate) use deletion_decision::DeletionDecision;
pub(crate) use evaluate_deletion::evaluate_deletion;
pub(crate) use logical_prune_plan::LogicalPrunePlan;
pub(crate) use logical_prune_plan_options::LogicalPrunePlanOptions;
pub(crate) use minio_logical_prune_options::MinioLogicalPruneOptions;
pub(crate) use mongodb_logical_prune_options::MongoDbLogicalPruneOptions;
pub(crate) use mysql_logical_prune_options::MySqlLogicalPruneOptions;
pub(crate) use open_stored_backup_artifact::open_stored_backup_artifact;
pub(crate) use postgres_logical_prune_options::PostgresLogicalPruneOptions;
pub(crate) use prune_authorization::PruneAuthorization;
pub(crate) use prune_minio_logical_resource::prune_minio_logical_resource;
pub(crate) use prune_mongodb_logical_resource::prune_mongodb_logical_resource;
pub(crate) use prune_mysql_logical_resource::prune_mysql_logical_resource;
pub(crate) use prune_postgres_logical_resource::prune_postgres_logical_resource;
pub(crate) use prune_rabbitmq_logical_resource::prune_rabbitmq_logical_resource;
pub(crate) use prune_sql_server_logical_resource::prune_sql_server_logical_resource;
pub(crate) use rabbitmq_logical_prune_options::RabbitMqLogicalPruneOptions;
pub(crate) use resolve_data_lifecycle_strategy::resolve_data_lifecycle_strategy;
pub(crate) use restore_error::RestoreError;
pub(crate) use restore_target::RestoreTarget;
pub(crate) use restore_target_error::RestoreTargetError;
pub(crate) use restore_verified_backup::restore_verified_backup;
pub(crate) use sql_server_logical_prune_options::SqlServerLogicalPruneOptions;
pub(crate) use store_backup_artifact::{
    store_backup_artifact, store_backup_artifact_for_identity, store_backup_artifact_from_reader,
};
pub(crate) use store_backup_artifact_from_async_reader::store_backup_artifact_from_async_reader;
pub(crate) use stored_backup_artifact::StoredBackupArtifact;
pub(crate) use verified_backup_evidence::VerifiedBackupEvidence;

/// Seven days before orphaned disposable containers are automatically removed.
pub(crate) const DEFAULT_ORPHAN_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;
pub(crate) use verify_backup_artifact::verify_backup_artifact;
pub(crate) use verify_stored_backup_artifact::verify_stored_backup_artifact;

/// Transitional name retained while the daemon prune coordinator becomes service-neutral.
pub(crate) type PostgresLogicalPrunePlan = LogicalPrunePlan;
/// Transitional options name for the existing PostgreSQL daemon coordinator.
pub(crate) type PostgresLogicalPrunePlanOptions<'state> = LogicalPrunePlanOptions<'state>;
