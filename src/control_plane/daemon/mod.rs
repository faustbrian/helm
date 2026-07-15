mod active_migration_decision;
mod active_postgres_prune;
mod active_project_backup;
mod active_project_command;
mod active_project_log_session;
mod active_project_restore;
mod benchmark_snapshot_provider;
#[cfg(unix)]
mod bollard_unix_engine_connector;
mod collect_benchmark_snapshot;
mod daemon_iteration_result;
mod daemon_request_dispatch_options;
mod daemon_shutdown_signal;
#[cfg(unix)]
mod default_unix_daemon_runtime_directory;
mod discover_project_sources;
mod discovery_reconciliation_error;
mod discovery_reconciliation_result;
mod discovery_scan_reason;
mod discovery_scheduler;
mod discovery_scheduler_error;
mod discovery_scheduler_options;
mod dispatch_daemon_request;
mod engine_benchmark_snapshot_provider;
mod engine_connection_outcome;
mod engine_connection_supervisor;
mod engine_connection_supervisor_error;
mod engine_connector;
mod engine_event_observation;
mod engine_event_subscription;
mod engine_image_reference_resolution;
mod engine_reconciliation_plan;
mod engine_reconciliation_plan_error;
mod engine_reconciliation_plan_options;
mod engine_reconciliation_schedule;
mod execute_minio_project_restore;
mod execute_project_logs;
mod execute_project_volume_restore;
mod execute_queued_migration_decision;
mod execute_queued_postgres_prune;
mod execute_queued_project_backup;
mod execute_queued_project_command;
mod execute_queued_project_restore;
mod execute_rabbitmq_project_restore;
mod execute_redis_project_restore;
mod execute_scheduled_project_command;
mod failed_event_json;
mod filesystem_event_watcher;
mod filesystem_event_watcher_error;
mod finalize_installation_deletion;
mod image_reference_resolution;
mod initialize_default_installation;
mod installation_initialization_error;
mod invalidate_engine_connection;
mod ipc;
mod is_valid_certificate_generation;
mod migration_decision_execution_options;
mod migration_decision_execution_result;
mod migration_decision_queue;
mod migration_decision_queue_error;
mod persisted_project_command;
mod plan_engine_reconciliation;
mod plan_postgres_prune;
mod postgres_prune_execution_options;
mod postgres_prune_execution_result;
mod postgres_prune_queue;
mod postgres_prune_queue_error;
mod project_backup_execution_options;
mod project_backup_execution_result;
mod project_backup_queue;
mod project_backup_queue_error;
mod project_command_execution_options;
mod project_command_execution_result;
mod project_command_queue;
mod project_command_queue_error;
mod project_discovery_error;
mod project_discovery_issue;
mod project_discovery_options;
mod project_discovery_report;
mod project_log_buffer;
mod project_log_buffer_error;
mod project_log_message;
mod project_log_request;
mod project_log_session_registry;
mod project_log_session_registry_error;
mod project_log_target;
mod project_restore_execution_options;
mod project_restore_execution_result;
mod project_restore_queue;
mod project_restore_queue_error;
mod project_restore_target_plan;
mod project_service_provisioning_registry;
mod publish_migration_decision_result;
mod publish_postgres_prune_result;
mod publish_project_backup_result;
mod publish_project_command_result;
mod publish_project_restore_result;
mod queue_next_installation_deletion_prune;
mod queued_migration_decision;
mod queued_postgres_prune;
mod queued_project_backup;
mod queued_project_command;
mod queued_project_restore;
mod reconcile_watched_roots;
mod record_discovery_diagnostics;
mod record_ipc_event;
mod requires_engine_reconciliation;
mod requires_followup_reconciliation;
mod resource_health;
mod resource_health_registry;
mod resource_health_registry_error;
mod restore_daemon_operation_queues;
mod restore_discovery_diagnostics;
mod retained_project_status;
mod retry_backoff;
mod retry_backoff_error;
mod retry_backoff_options;
mod retry_delay;
mod retry_failed_installation_deletion_prune;
mod run_bounded_independent_reconciliation;
#[cfg(unix)]
mod run_unix_daemon_watch;
#[cfg(unix)]
mod run_unix_daemon_watch_with_resolver;
mod scheduled_command_clock;
mod singleton_lease;
mod singleton_lease_error;
#[cfg(unix)]
mod unix_daemon_installation_deletion;
#[cfg(unix)]
mod unix_daemon_migration_decisions;
#[cfg(unix)]
mod unix_daemon_postgres_prunes;
#[cfg(unix)]
mod unix_daemon_project_backups;
#[cfg(unix)]
mod unix_daemon_project_commands;
mod unix_daemon_project_logs;
#[cfg(unix)]
mod unix_daemon_project_restores;
#[cfg(unix)]
mod unix_daemon_runtime;
#[cfg(unix)]
mod unix_daemon_runtime_error;
#[cfg(unix)]
mod unix_daemon_runtime_options;
#[cfg(unix)]
mod unix_daemon_scheduled_commands;
#[cfg(unix)]
mod unix_daemon_shutdown_drain;
#[cfg(unix)]
mod unix_daemon_shutdown_signal;
#[cfg(unix)]
mod unix_daemon_watch_options;
mod validate_project_workload_adoption;

pub(crate) use active_migration_decision::ActiveMigrationDecision;
pub(crate) use active_postgres_prune::ActivePostgresPrune;
pub(crate) use active_project_backup::ActiveProjectBackup;
pub(crate) use active_project_command::ActiveProjectCommand;
pub(crate) use active_project_log_session::ActiveProjectLogSession;
pub(crate) use active_project_restore::ActiveProjectRestore;
pub(crate) use scheduled_command_clock::ScheduledCommandClock;
pub(crate) use singleton_lease::SingletonLease;
pub(crate) use singleton_lease_error::SingletonLeaseError;

#[cfg(test)]
mod configuration_lifecycle_tests;
#[cfg(test)]
mod tests;
pub(crate) use benchmark_snapshot_provider::BenchmarkSnapshotProvider;
#[cfg(unix)]
pub(crate) use bollard_unix_engine_connector::BollardUnixEngineConnector;
pub(crate) use collect_benchmark_snapshot::collect_benchmark_snapshot;
pub(crate) use daemon_iteration_result::DaemonIterationResult;
pub(crate) use daemon_request_dispatch_options::DaemonRequestDispatchOptions;
pub(crate) use daemon_shutdown_signal::DaemonShutdownSignal;
#[cfg(unix)]
pub(crate) use default_unix_daemon_runtime_directory::default_unix_daemon_runtime_directory;
pub(crate) use discover_project_sources::discover_project_sources;
pub(crate) use discovery_reconciliation_error::DiscoveryReconciliationError;
pub(crate) use discovery_reconciliation_result::DiscoveryReconciliationResult;
pub(crate) use discovery_scan_reason::DiscoveryScanReason;
pub(crate) use discovery_scheduler::DiscoveryScheduler;
pub(crate) use discovery_scheduler_error::DiscoverySchedulerError;
pub(crate) use discovery_scheduler_options::DiscoverySchedulerOptions;
pub(crate) use dispatch_daemon_request::dispatch_daemon_request;
pub(crate) use engine_benchmark_snapshot_provider::EngineBenchmarkSnapshotProvider;
pub(crate) use engine_connection_outcome::EngineConnectionOutcome;
pub(crate) use engine_connection_supervisor::EngineConnectionSupervisor;
pub(crate) use engine_connection_supervisor_error::EngineConnectionSupervisorError;
pub(crate) use engine_connector::{EngineConnectionFuture, EngineConnector};
pub(crate) use engine_event_observation::EngineEventObservation;
pub(crate) use engine_event_subscription::EngineEventSubscription;
pub(crate) use engine_image_reference_resolution::EngineImageReferenceResolution;
pub(crate) use engine_reconciliation_plan::EngineReconciliationPlan;
pub(crate) use engine_reconciliation_plan_error::EngineReconciliationPlanError;
pub(crate) use engine_reconciliation_plan_options::EngineReconciliationPlanOptions;
pub(crate) use engine_reconciliation_schedule::EngineReconciliationSchedule;
pub(crate) use execute_minio_project_restore::execute_minio_project_restore;
pub(crate) use execute_project_logs::execute_project_logs;
pub(crate) use execute_project_volume_restore::execute_project_volume_restore;
pub(crate) use execute_queued_migration_decision::execute_queued_migration_decision;
pub(crate) use execute_queued_postgres_prune::execute_queued_postgres_prune;
pub(crate) use execute_queued_project_backup::execute_queued_project_backup;
pub(crate) use execute_queued_project_command::execute_queued_project_command;
pub(crate) use execute_queued_project_restore::execute_queued_project_restore;
pub(crate) use execute_rabbitmq_project_restore::execute_rabbitmq_project_restore;
pub(crate) use execute_redis_project_restore::execute_redis_project_restore;
pub(crate) use execute_scheduled_project_command::execute_scheduled_project_command;
pub(crate) use failed_event_json::failed_event_json;
pub(crate) use filesystem_event_watcher::FilesystemEventWatcher;
pub(crate) use filesystem_event_watcher_error::FilesystemEventWatcherError;
pub(crate) use finalize_installation_deletion::finalize_installation_deletion;
pub(crate) use image_reference_resolution::ImageReferenceResolution;
pub(crate) use initialize_default_installation::initialize_default_installation;
pub(crate) use installation_initialization_error::InstallationInitializationError;
pub(crate) use invalidate_engine_connection::invalidate_engine_connection;
#[cfg(unix)]
pub(crate) use ipc::{
    IpcBenchmarkSnapshot, IpcDataLifecycle, IpcDiagnostic, IpcEvent, IpcEventJournal, IpcEventKind,
    IpcInstallationDeletionStatus, IpcInstallationLifecycle, IpcLogChunk, IpcLogSessionState,
    IpcManagedEnvironment, IpcMigrationDecision, IpcNodePackageManager, IpcOutcome,
    IpcOutputStream, IpcPayload, IpcPhpTool, IpcProjectCommand, IpcProjectStatus, IpcRequest,
    IpcResourceHealth, IpcResourceLifecycle, IpcResourceStatus, IpcResponse, IpcResult,
    send_unix_request,
};
pub(crate) use is_valid_certificate_generation::is_valid_certificate_generation;
pub(crate) use migration_decision_execution_options::MigrationDecisionExecutionOptions;
pub(crate) use migration_decision_execution_result::MigrationDecisionExecutionResult;
pub(crate) use migration_decision_queue::MigrationDecisionQueue;
pub(crate) use migration_decision_queue_error::MigrationDecisionQueueError;
pub(crate) use persisted_project_command::PersistedProjectCommand;
pub(crate) use plan_engine_reconciliation::plan_engine_reconciliation;
pub(crate) use plan_postgres_prune::{build_postgres_prune_plan, plan_postgres_prune};
pub(crate) use postgres_prune_execution_options::PostgresPruneExecutionOptions;
pub(crate) use postgres_prune_execution_result::PostgresPruneExecutionResult;
pub(crate) use postgres_prune_queue::PostgresPruneQueue;
pub(crate) use postgres_prune_queue_error::PostgresPruneQueueError;
pub(crate) use project_backup_execution_options::ProjectBackupExecutionOptions;
pub(crate) use project_backup_execution_result::ProjectBackupExecutionResult;
pub(crate) use project_backup_queue::ProjectBackupQueue;
pub(crate) use project_backup_queue_error::ProjectBackupQueueError;
pub(crate) use project_command_execution_options::ProjectCommandExecutionOptions;
pub(crate) use project_command_execution_result::ProjectCommandExecutionResult;
pub(crate) use project_command_queue::ProjectCommandQueue;
pub(crate) use project_command_queue_error::ProjectCommandQueueError;
pub(crate) use project_discovery_error::ProjectDiscoveryError;
pub(crate) use project_discovery_issue::ProjectDiscoveryIssue;
pub(crate) use project_discovery_options::ProjectDiscoveryOptions;
pub(crate) use project_discovery_report::ProjectDiscoveryReport;
pub(crate) use project_log_buffer::ProjectLogBuffer;
pub(crate) use project_log_buffer_error::ProjectLogBufferError;
pub(crate) use project_log_message::ProjectLogMessage;
pub(crate) use project_log_request::ProjectLogRequest;
pub(crate) use project_log_session_registry::ProjectLogSessionRegistry;
pub(crate) use project_log_session_registry_error::ProjectLogSessionRegistryError;
pub(crate) use project_log_target::ProjectLogTarget;
pub(crate) use project_restore_execution_options::ProjectRestoreExecutionOptions;
pub(crate) use project_restore_execution_result::ProjectRestoreExecutionResult;
pub(crate) use project_restore_queue::ProjectRestoreQueue;
pub(crate) use project_restore_queue_error::ProjectRestoreQueueError;
pub(crate) use project_restore_target_plan::ProjectRestoreTargetPlan;
pub(crate) use project_service_provisioning_registry::ProjectServiceProvisioningRegistry;
pub(crate) use publish_migration_decision_result::publish_migration_decision_result;
pub(crate) use publish_postgres_prune_result::publish_postgres_prune_result;
pub(crate) use publish_project_backup_result::publish_project_backup_result;
pub(crate) use publish_project_command_result::publish_project_command_result;
pub(crate) use publish_project_restore_result::publish_project_restore_result;
pub(crate) use queue_next_installation_deletion_prune::queue_next_installation_deletion_prune;
pub(crate) use queued_migration_decision::QueuedMigrationDecision;
pub(crate) use queued_postgres_prune::QueuedPostgresPrune;
pub(crate) use queued_project_backup::QueuedProjectBackup;
pub(crate) use queued_project_command::QueuedProjectCommand;
pub(crate) use queued_project_restore::{QueuedProjectRestore, QueuedProjectRestoreOptions};
pub(crate) use reconcile_watched_roots::reconcile_watched_roots;
pub(crate) use record_discovery_diagnostics::record_discovery_diagnostics;
pub(crate) use requires_engine_reconciliation::requires_engine_reconciliation;
pub(crate) use requires_followup_reconciliation::requires_followup_reconciliation;
pub(crate) use resource_health::ResourceHealth;
pub(crate) use resource_health_registry::ResourceHealthRegistry;
pub(crate) use resource_health_registry_error::ResourceHealthRegistryError;
pub(crate) use restore_daemon_operation_queues::restore_daemon_operation_queues;
pub(crate) use restore_discovery_diagnostics::restore_discovery_diagnostics;
pub(crate) use retained_project_status::retained_project_status;
pub(crate) use retry_backoff::RetryBackoff;
pub(crate) use retry_backoff_error::RetryBackoffError;
pub(crate) use retry_backoff_options::RetryBackoffOptions;
pub(crate) use retry_delay::RetryDelay;
pub(crate) use retry_failed_installation_deletion_prune::retry_failed_installation_deletion_prune;
pub(crate) use run_bounded_independent_reconciliation::run_bounded_independent_reconciliation;
#[cfg(unix)]
pub(crate) use run_unix_daemon_watch::run_unix_daemon_watch;
#[cfg(unix)]
pub(crate) use run_unix_daemon_watch_with_resolver::run_unix_daemon_watch_with_resolver;
#[cfg(unix)]
pub(crate) use unix_daemon_runtime::UnixDaemonRuntime;
#[cfg(unix)]
pub(crate) use unix_daemon_runtime_error::UnixDaemonRuntimeError;
#[cfg(unix)]
pub(crate) use unix_daemon_runtime_options::UnixDaemonRuntimeOptions;
#[cfg(unix)]
pub(crate) use unix_daemon_shutdown_signal::UnixDaemonShutdownSignal;
#[cfg(unix)]
pub(crate) use unix_daemon_watch_options::UnixDaemonWatchOptions;
pub(crate) use validate_project_workload_adoption::validate_project_workload_adoption;
