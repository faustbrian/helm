mod frame;
mod ipc_benchmark_container_metrics;
mod ipc_benchmark_container_metrics_options;
mod ipc_benchmark_snapshot;
mod ipc_benchmark_tcp_port;
mod ipc_data_lifecycle;
mod ipc_error;
mod ipc_event;
mod ipc_event_journal;
mod ipc_event_journal_error;
mod ipc_event_kind;
mod ipc_installation_deletion_plan;
mod ipc_log_chunk;
mod ipc_log_session_state;
mod ipc_managed_environment;
mod ipc_migration_decision;
mod ipc_migration_status;
mod ipc_node_package_manager;
mod ipc_output_stream;
mod ipc_payload;
mod ipc_php_tool;
mod ipc_postgres_prune_plan;
mod ipc_postgres_prune_plan_options;
mod ipc_project_command;
mod ipc_project_status;
mod ipc_recovery_point;
mod ipc_request;
mod ipc_resource_health;
mod ipc_resource_lifecycle;
mod ipc_resource_status;
mod ipc_response;
mod ipc_result;
#[cfg(unix)]
mod send_unix_request;
#[cfg(unix)]
mod unix_ipc_listener;

pub(crate) use frame::{decode_request_frame, decode_response_frame, encode_frame};
pub(crate) use ipc_benchmark_container_metrics::IpcBenchmarkContainerMetrics;
pub(crate) use ipc_benchmark_container_metrics_options::IpcBenchmarkContainerMetricsOptions;
pub(crate) use ipc_benchmark_snapshot::IpcBenchmarkSnapshot;
pub(crate) use ipc_benchmark_tcp_port::IpcBenchmarkTcpPort;
pub(crate) use ipc_data_lifecycle::IpcDataLifecycle;
pub(crate) use ipc_error::IpcError;
pub(crate) use ipc_event::IpcEvent;
pub(crate) use ipc_event_journal::IpcEventJournal;
pub(crate) use ipc_event_journal_error::IpcEventJournalError;
pub(crate) use ipc_event_kind::IpcEventKind;
pub(crate) use ipc_installation_deletion_plan::IpcInstallationDeletionPlan;
pub(crate) use ipc_log_chunk::IpcLogChunk;
pub(crate) use ipc_log_session_state::IpcLogSessionState;
pub(crate) use ipc_managed_environment::IpcManagedEnvironment;
pub(crate) use ipc_migration_decision::IpcMigrationDecision;
pub(crate) use ipc_migration_status::IpcMigrationStatus;
pub(crate) use ipc_node_package_manager::IpcNodePackageManager;
pub(crate) use ipc_output_stream::IpcOutputStream;
pub(crate) use ipc_payload::IpcPayload;
pub(crate) use ipc_php_tool::IpcPhpTool;
pub(crate) use ipc_postgres_prune_plan::IpcPostgresPrunePlan;
pub(crate) use ipc_postgres_prune_plan_options::IpcPostgresPrunePlanOptions;
pub(crate) use ipc_project_command::IpcProjectCommand;
pub(crate) use ipc_project_status::IpcProjectStatus;
pub(crate) use ipc_recovery_point::IpcRecoveryPoint;
pub(crate) use ipc_request::{IPC_PROTOCOL_VERSION, IpcRequest};
pub(crate) use ipc_resource_health::IpcResourceHealth;
pub(crate) use ipc_resource_lifecycle::IpcResourceLifecycle;
pub(crate) use ipc_resource_status::IpcResourceStatus;
pub(crate) use ipc_response::{IpcDiagnostic, IpcOutcome, IpcResponse};
pub(crate) use ipc_result::IpcResult;
#[cfg(unix)]
pub(crate) use send_unix_request::send_unix_request;
#[cfg(unix)]
pub(crate) use unix_ipc_listener::UnixIpcListener;

#[cfg(test)]
mod tests;
