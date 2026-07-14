use super::{
    ActiveMigrationDecision, ActivePostgresPrune, ActiveProjectBackup, ActiveProjectCommand,
    ActiveProjectLogSession, ActiveProjectRestore, BollardUnixEngineConnector,
    DaemonIterationResult, DaemonRequestDispatchOptions, DiscoveryScheduler,
    EngineBenchmarkSnapshotProvider, EngineConnectionOutcome, EngineConnectionSupervisor,
    EngineImageReferenceResolution, EngineReconciliationPlanOptions, EngineReconciliationSchedule,
    EngineV7ProjectInventoryProvider, FilesystemEventWatcher, ImageReferenceResolution,
    IpcEventJournal, MigrationDecisionQueue, PostgresPruneQueue, ProjectBackupQueue,
    ProjectCommandQueue, ProjectLogSessionRegistry, ProjectRestoreQueue, ResourceHealthRegistry,
    RetryBackoff, RetryBackoffOptions, SingletonLease, UnixDaemonRuntimeError,
    UnixDaemonRuntimeOptions, dispatch_daemon_request, initialize_default_installation,
    invalidate_engine_connection, plan_engine_reconciliation, reconcile_watched_roots,
    requires_followup_reconciliation, restore_daemon_operation_queues,
    validate_project_workload_adoption,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::UnixIpcListener;
use crate::control_plane::engine::NetworkCreateOptions;
use crate::control_plane::gateway::{
    GatewayPlaneOptions, GatewayReconcileOptions, GatewayRuntimeAssetOptions,
    SystemGatewayPortProbe, prepare_gateway_runtime_assets, reconcile_gateway_plane,
};
use crate::control_plane::network::{
    GlobalNetworkReconcileAction, GlobalNetworkReconcileError, GlobalNetworkReconcileOptions,
    global_network_request, reconcile_global_network,
};
use crate::control_plane::retention::DEFAULT_ORPHAN_RETENTION_SECONDS;
use crate::control_plane::shared_infrastructure::{
    OsCredentialEntropy, PreparedSharedInstance, SharedInfrastructureReconcileError,
    SharedPreparationOptions, UnreferencedSharedServiceOptions, reconcile_prepared_shared_instance,
    resolve_execution_shared_instances, stop_unreferenced_shared_services,
};
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use crate::control_plane::state::{InstallationLifecycle, SqliteStateStore, StateStore};
use crate::control_plane::workload::{
    DisposableContainerGarbageCollectionOptions, OrphanedProjectWorkloadOptions,
    ProjectVolumeReconcileOptions, WorkloadReconcileError, WorkloadReconcileOptions,
    garbage_collect_disposable_containers, project_volume_resource_record,
    reconcile_project_application, reconcile_project_process, reconcile_project_service,
    reconcile_project_volume, remove_stale_ephemeral_services, stop_orphaned_project_workloads,
    workload_resource_record,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// One authoritative Unix daemon owning state, scheduling, lease, and IPC.
pub(crate) struct UnixDaemonRuntime {
    _lease: SingletonLease,
    listener: UnixIpcListener,
    filesystem_watcher: FilesystemEventWatcher,
    pub(super) engine_runtime: tokio::runtime::Runtime,
    pub(super) engine_connection: EngineConnectionSupervisor<BollardUnixEngineConnector>,
    pub(super) runtime_directory: PathBuf,
    pub(super) global_network_request: NetworkCreateOptions,
    pub(super) engine_reconciliation: EngineReconciliationSchedule,
    pub(super) event_journal: IpcEventJournal,
    pub(super) project_commands: ProjectCommandQueue,
    pub(super) project_backups: ProjectBackupQueue,
    pub(super) postgres_prunes: PostgresPruneQueue,
    pub(super) project_restores: ProjectRestoreQueue,
    pub(super) migration_decisions: MigrationDecisionQueue,
    pub(super) project_logs: ProjectLogSessionRegistry,
    pub(super) resource_health: ResourceHealthRegistry,
    pub(super) active_project_logs: BTreeMap<String, ActiveProjectLogSession>,
    pub(super) active_project_command: Option<ActiveProjectCommand>,
    pub(super) active_project_backup: Option<ActiveProjectBackup>,
    pub(super) active_project_restore: Option<ActiveProjectRestore>,
    pub(super) active_postgres_prune: Option<ActivePostgresPrune>,
    pub(super) active_migration_decision: Option<ActiveMigrationDecision>,
    pub(super) control_plane: ControlPlane<SqliteStateStore>,
    scheduler: DiscoveryScheduler,
    pub(super) options: UnixDaemonRuntimeOptions,
}

impl UnixDaemonRuntime {
    /// Acquires exclusive ownership and opens every durable runtime boundary.
    pub(crate) fn new(
        options: UnixDaemonRuntimeOptions,
        now: Instant,
    ) -> Result<Self, UnixDaemonRuntimeError> {
        validate_options(&options)?;
        let runtime_directory = options
            .socket_path
            .parent()
            .ok_or_else(|| invalid("daemon socket path must have a parent directory"))?
            .to_path_buf();
        prepare_runtime_directory(&runtime_directory)?;
        let lease = SingletonLease::acquire(&options.lease_path)?;
        remove_stale_socket(&options.socket_path)?;
        let listener = UnixIpcListener::bind(&options.socket_path)?;
        listener.set_nonblocking(true)?;
        let mut store = SqliteStateStore::open_with_backups(
            &options.state_database_path,
            &runtime_directory.join("state-backups"),
            unix_time_seconds(),
        )?;
        let (
            project_commands,
            project_backups,
            postgres_prunes,
            project_restores,
            migration_decisions,
        ) = restore_daemon_operation_queues(&mut store, unix_time_seconds())?;
        let event_journal = IpcEventJournal::restore(store.daemon_events()?)?;
        let installation = initialize_default_installation(&mut store)?;
        let filesystem_watcher = FilesystemEventWatcher::new(&store.watched_roots()?)?;
        let engine_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(UnixDaemonRuntimeError::AsyncRuntime)?;
        let engine_retry = RetryBackoff::new(
            format!("engine:{}", installation.engine_endpoint()),
            RetryBackoffOptions::new(Duration::from_millis(500), Duration::from_secs(30))?,
        )?;
        let engine_connection = EngineConnectionSupervisor::new(
            BollardUnixEngineConnector::new(Duration::from_secs(1)),
            installation.engine_endpoint().into(),
            engine_retry,
        )?;
        let global_network_request = global_network_request(installation.installation_id())?;
        let scheduler = DiscoveryScheduler::new(now, options.scheduler_options);

        Ok(Self {
            _lease: lease,
            listener,
            filesystem_watcher,
            engine_runtime,
            engine_connection,
            runtime_directory,
            global_network_request,
            engine_reconciliation: EngineReconciliationSchedule::default(),
            event_journal,
            project_commands,
            project_backups,
            postgres_prunes,
            project_restores,
            migration_decisions,
            project_logs: ProjectLogSessionRegistry::default(),
            resource_health: ResourceHealthRegistry::default(),
            active_project_logs: BTreeMap::new(),
            active_project_command: None,
            active_project_backup: None,
            active_project_restore: None,
            active_postgres_prune: None,
            active_migration_decision: None,
            control_plane: ControlPlane::new(store),
            scheduler,
            options,
        })
    }

    /// Performs due reconciliation and serves at most one pending IPC request.
    pub(crate) fn run_iteration(
        &mut self,
        now: Instant,
        now_unix_seconds: i64,
    ) -> Result<DaemonIterationResult, UnixDaemonRuntimeError> {
        if now_unix_seconds < 0 {
            return Err(invalid("daemon wall-clock time must not be negative"));
        }
        let reconciliation_frozen = matches!(
            self.control_plane.installation_lifecycle()?,
            Some(InstallationLifecycle::Deleting | InstallationLifecycle::Deleted)
        );
        if self.filesystem_watcher.take_change()? && !reconciliation_frozen {
            self.record_filesystem_event(now);
        }

        let scan_reason = (!reconciliation_frozen)
            .then(|| self.scheduler.take_due(now))
            .flatten();
        let reconciliation = scan_reason
            .map(|_reason| {
                reconcile_watched_roots(
                    &mut self.control_plane,
                    self.options.discovery_options,
                    now_unix_seconds,
                )
            })
            .transpose()?;
        let mut image_reference_resolution = self
            .engine_connection
            .engine()
            .cloned()
            .map(|engine| EngineImageReferenceResolution::new(&self.engine_runtime, engine));
        let mut benchmark_snapshot = self.engine_connection.engine().cloned().map(|engine| {
            EngineBenchmarkSnapshotProvider::new(
                &self.engine_runtime,
                engine,
                self.global_network_request
                    .metadata()
                    .installation_id()
                    .to_owned(),
                self.global_network_request.metadata().schema_version(),
            )
        });
        let mut v7_project_inventory = self
            .engine_connection
            .engine()
            .cloned()
            .map(|engine| EngineV7ProjectInventoryProvider::new(&self.engine_runtime, engine));
        let request = self.listener.try_serve_next(|request| {
            dispatch_daemon_request(DaemonRequestDispatchOptions {
                control_plane: &mut self.control_plane,
                discovery_options: self.options.discovery_options,
                request,
                event_journal: &mut self.event_journal,
                project_commands: &mut self.project_commands,
                project_backups: &mut self.project_backups,
                postgres_prunes: &mut self.postgres_prunes,
                project_restores: &mut self.project_restores,
                migration_decisions: &mut self.migration_decisions,
                project_logs: &mut self.project_logs,
                resource_health: &self.resource_health,
                benchmark_snapshot: benchmark_snapshot.as_mut().map(|provider| {
                    let provider: &mut dyn super::BenchmarkSnapshotProvider = provider;
                    provider
                }),
                image_reference_resolution: image_reference_resolution.as_mut().map(|resolver| {
                    let resolver: &mut dyn ImageReferenceResolution = resolver;
                    resolver
                }),
                v7_project_inventory: v7_project_inventory.as_mut().map(|provider| {
                    let provider: &mut dyn super::V7ProjectInventoryProvider = provider;
                    provider
                }),
                now_unix_seconds,
            })
        })?;
        if request
            .as_ref()
            .is_some_and(requires_followup_reconciliation)
        {
            self.scheduler.record_filesystem_event(now);
        }
        let installation_reconciliation_frozen = matches!(
            self.control_plane.installation_lifecycle()?,
            Some(InstallationLifecycle::Deleting | InstallationLifecycle::Deleted)
        );

        Ok(DaemonIterationResult::new(
            scan_reason,
            reconciliation,
            request,
            installation_reconciliation_frozen,
        ))
    }

    /// Records a native filesystem notification for debounced convergence.
    pub(crate) fn record_filesystem_event(&mut self, now: Instant) {
        self.scheduler.record_filesystem_event(now);
    }

    /// Runs until the process is stopped by the per-user service manager.
    #[expect(
        clippy::infinite_loop,
        reason = "the singleton daemon is a login-lifetime service"
    )]
    pub(crate) fn run_forever(&mut self) {
        loop {
            let now = Instant::now();
            let now_unix_seconds = unix_time_seconds();
            match self.run_iteration(now, now_unix_seconds) {
                Ok(iteration) => {
                    if let Some(reconciliation) = iteration.reconciliation() {
                        if let Err(error) = self.engine_reconciliation.observe(reconciliation) {
                            tracing::error!(
                                error = %error,
                                "validated registry could not resolve to an Engine plan"
                            );
                        }
                    }
                    if !iteration.installation_reconciliation_frozen()
                        && !self.has_active_project_command()
                        && !self.has_active_project_backup()
                        && !self.has_active_project_restore()
                        && !self.has_active_postgres_prune()
                        && !self.has_active_migration_decision()
                        && self.engine_reconciliation.may_reconcile()
                    {
                        self.reconcile_engine_plane(now, now_unix_seconds);
                    }
                    self.drive_project_commands(now, now_unix_seconds);
                    self.drive_project_backups(now, now_unix_seconds);
                    self.drive_postgres_prunes(now, now_unix_seconds);
                    self.drive_project_restores(now, now_unix_seconds);
                    self.drive_migration_decisions(now, now_unix_seconds);
                    self.drive_project_logs(now);
                    self.drive_installation_deletion(now, now_unix_seconds);
                }
                Err(error) => {
                    tracing::error!(error = %error, "singleton daemon iteration failed");
                    self.scheduler.record_filesystem_event(Instant::now());
                }
            }
            let until_scan = self
                .scheduler
                .next_deadline()
                .saturating_duration_since(now);
            std::thread::sleep(self.options.idle_poll_interval.min(until_scan));
        }
    }

    fn reconcile_engine_plane(&mut self, now: Instant, observed_at_unix_seconds: i64) {
        match self
            .engine_runtime
            .block_on(self.engine_connection.poll(now))
        {
            EngineConnectionOutcome::Unavailable { retry, detail } => {
                tracing::debug!(
                    attempt = retry.attempt(),
                    retry_milliseconds = retry.duration().as_millis(),
                    error = %detail,
                    "selected Docker Engine is unavailable; retry scheduled"
                );

                return;
            }
            EngineConnectionOutcome::Connected | EngineConnectionOutcome::BackingOff { .. } => {}
        }

        if !self.engine_connection.is_connected() || !self.engine_reconciliation.is_due() {
            return;
        }
        let Some(execution) = self.engine_reconciliation.execution_plan() else {
            self.engine_reconciliation.complete();
            tracing::error!("Engine reconciliation was due without a resolved execution plan");

            return;
        };
        let durable_resources = match self.control_plane.resources() {
            Ok(resources) => resources,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "durable resource inventory blocked");

                return;
            }
        };
        if let Err(error) = validate_project_workload_adoption(execution, &durable_resources) {
            self.engine_reconciliation.complete();
            tracing::error!(error = %error, "project workload adoption required");

            return;
        }
        let platform = match runtime_linux_platform() {
            Ok(platform) => platform,
            Err(detail) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = detail, "Engine platform planning blocked");

                return;
            }
        };
        let shared = match resolve_execution_shared_instances(execution, platform) {
            Ok(shared) => shared,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "shared service demand planning blocked");

                return;
            }
        };
        let prepared_shared = match self.control_plane.prepare_shared(
            &shared,
            &OsCredentialEntropy,
            SharedPreparationOptions {
                installation_id: self.global_network_request.metadata().installation_id(),
                network_name: self.global_network_request.name(),
                schema_version: self.global_network_request.metadata().schema_version(),
                state_directory: &self.runtime_directory,
            },
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "shared infrastructure preparation blocked");

                return;
            }
        };
        let managed_environments = match merge_prepared_environments(execution, &prepared_shared) {
            Ok(environments) => environments,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error, "managed environment planning blocked");

                return;
            }
        };
        let prepared_shared_services = prepared_shared
            .iter()
            .flat_map(PreparedSharedInstance::service_identities)
            .collect::<Vec<_>>();
        let shared_routes = prepared_shared
            .iter()
            .flat_map(PreparedSharedInstance::routes)
            .collect::<Vec<_>>();
        let engine_plan = match plan_engine_reconciliation(EngineReconciliationPlanOptions {
            execution,
            prepared_shared_services: &prepared_shared_services,
            shared_routes: &shared_routes,
            managed_environments: &managed_environments,
            durable_resources: &durable_resources,
            installation_id: self.global_network_request.metadata().installation_id(),
            schema_version: self.global_network_request.metadata().schema_version(),
            platform,
            network_name: self.global_network_request.name(),
            internal_http_port: 8080,
        }) {
            Ok(plan) => plan,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "complete Engine reconciliation planning blocked");

                return;
            }
        };
        let Some(engine) = self.engine_connection.engine_mut() else {
            return;
        };
        let result = self.engine_runtime.block_on(reconcile_global_network(
            engine,
            GlobalNetworkReconcileOptions {
                request: &self.global_network_request,
                installation_id: self.global_network_request.metadata().installation_id(),
                schema_version: self.global_network_request.metadata().schema_version(),
            },
        ));

        let network = match result {
            Ok(result) => result,
            Err(GlobalNetworkReconcileError::EngineUnavailable { action, detail }) => {
                let retry = invalidate_engine_connection(
                    &mut self.engine_connection,
                    &mut self.resource_health,
                    now,
                );
                tracing::debug!(
                    attempt = retry.attempt(),
                    retry_milliseconds = retry.duration().as_millis(),
                    action,
                    error = detail,
                    "global network reconciliation lost the selected Engine; retry scheduled"
                );

                return;
            }
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "global Engine network reconciliation blocked");

                return;
            }
        };
        if network.action() == GlobalNetworkReconcileAction::Created {
            tracing::info!(
                network_id = network.network().id().as_str(),
                "created the global Stackctl Engine network"
            );
        }

        let removed_ephemeral = self
            .engine_runtime
            .block_on(remove_stale_ephemeral_services(
                engine,
                self.global_network_request.metadata().installation_id(),
                self.global_network_request.metadata().schema_version(),
            ));
        match removed_ephemeral {
            Ok(removed) if removed > 0 => {
                tracing::info!(removed, "removed interrupted ephemeral services");
            }
            Ok(_) => {}
            Err(error @ WorkloadReconcileError::Engine { .. }) => {
                let retry = invalidate_engine_connection(
                    &mut self.engine_connection,
                    &mut self.resource_health,
                    now,
                );
                tracing::debug!(
                    attempt = retry.attempt(),
                    retry_milliseconds = retry.duration().as_millis(),
                    error = %error,
                    "ephemeral service recovery lost the selected Engine; retry scheduled"
                );

                return;
            }
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "ephemeral service recovery blocked");

                return;
            }
        }

        let mut health_snapshot = ResourceHealthRegistry::default();
        let mut physical_resources = Vec::new();
        let mut provisioned = execution
            .services()
            .iter()
            .map(|service| (service.project().as_str().to_owned(), Vec::new()))
            .collect::<BTreeMap<String, Vec<_>>>();
        for prepared in &prepared_shared {
            let logical = self
                .engine_runtime
                .block_on(reconcile_prepared_shared_instance(
                    engine,
                    prepared,
                    self.global_network_request.metadata().installation_id(),
                    self.global_network_request.metadata().schema_version(),
                ));
            let logical = match logical {
                Ok(logical) => logical,
                Err(error @ SharedInfrastructureReconcileError::Engine { .. }) => {
                    let retry = invalidate_engine_connection(
                        &mut self.engine_connection,
                        &mut self.resource_health,
                        now,
                    );
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %error,
                        "shared infrastructure reconciliation lost the selected Engine; retry scheduled"
                    );

                    return;
                }
                Err(error) => {
                    self.engine_reconciliation.complete();
                    tracing::error!(error = %error, "shared infrastructure instance blocked");

                    return;
                }
            };
            let container_resource = logical
                .physical_resources()
                .first()
                .expect("shared reconciliation always returns its container first");
            if let Err(error) = health_snapshot.record(
                container_resource.resource_id(),
                logical.health(),
                observed_at_unix_seconds,
            ) {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "shared health snapshot publication blocked");

                return;
            }
            physical_resources.extend(logical.physical_resources().iter().cloned());
            for logical in logical.logical_resources() {
                provisioned
                    .entry(logical.project_id().to_owned())
                    .or_default()
                    .push(logical.clone());
            }
        }
        let reconciled_at_unix_seconds = observed_at_unix_seconds;
        if let Err(error) = self
            .control_plane
            .record_resources(&physical_resources, reconciled_at_unix_seconds)
        {
            self.engine_reconciliation.complete();
            tracing::error!(error = %error, "physical shared ownership publication blocked");

            return;
        }
        for (project_id, logical) in provisioned {
            let Some(environment) = managed_environments
                .iter()
                .find(|environment| environment.project_id() == project_id.as_str())
            else {
                self.engine_reconciliation.complete();
                tracing::error!(
                    project = project_id,
                    "provisioned project environment is missing"
                );

                return;
            };
            if let Err(error) = self.control_plane.reconcile_logical_environment(
                &logical,
                environment,
                reconciled_at_unix_seconds,
            ) {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "shared tenant state publication blocked");

                return;
            }
        }

        let current_resources = match self.control_plane.resources() {
            Ok(resources) => resources,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "shared service ownership inventory blocked");

                return;
            }
        };
        let current_logical_resources = match self.control_plane.logical_resources() {
            Ok(resources) => resources,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "logical ownership inventory blocked");

                return;
            }
        };
        let stopped_shared = self
            .engine_runtime
            .block_on(stop_unreferenced_shared_services(
                engine,
                UnreferencedSharedServiceOptions {
                    resources: &current_resources,
                    logical_resources: &current_logical_resources,
                    installation_id: self.global_network_request.metadata().installation_id(),
                    schema_version: self.global_network_request.metadata().schema_version(),
                },
            ));
        match stopped_shared {
            Ok(stopped) if stopped > 0 => {
                tracing::info!(stopped, "stopped unreferenced shared services");
            }
            Ok(_) => {}
            Err(error @ SharedInfrastructureReconcileError::Engine { .. }) => {
                let retry = invalidate_engine_connection(
                    &mut self.engine_connection,
                    &mut self.resource_health,
                    now,
                );
                tracing::debug!(
                    attempt = retry.attempt(),
                    retry_milliseconds = retry.duration().as_millis(),
                    error = %error,
                    "shared service idling lost the selected Engine; retry scheduled"
                );

                return;
            }
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "shared service idling blocked");

                return;
            }
        }

        let stopped = self
            .engine_runtime
            .block_on(stop_orphaned_project_workloads(
                engine,
                OrphanedProjectWorkloadOptions {
                    resources: &durable_resources,
                    installation_id: self.global_network_request.metadata().installation_id(),
                    schema_version: self.global_network_request.metadata().schema_version(),
                },
            ));
        match stopped {
            Ok(stopped) if stopped > 0 => {
                tracing::info!(stopped, "stopped orphaned project workloads");
            }
            Ok(_) => {}
            Err(error @ WorkloadReconcileError::Engine { .. }) => {
                let retry = invalidate_engine_connection(
                    &mut self.engine_connection,
                    &mut self.resource_health,
                    now,
                );
                tracing::debug!(
                    attempt = retry.attempt(),
                    retry_milliseconds = retry.duration().as_millis(),
                    error = %error,
                    "orphan workload reconciliation lost the selected Engine; retry scheduled"
                );

                return;
            }
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "orphan workload reconciliation blocked");

                return;
            }
        }

        let retired = self
            .engine_runtime
            .block_on(garbage_collect_disposable_containers(
                engine,
                DisposableContainerGarbageCollectionOptions {
                    resources: &durable_resources,
                    installation_id: self.global_network_request.metadata().installation_id(),
                    schema_version: self.global_network_request.metadata().schema_version(),
                    now_unix_seconds: observed_at_unix_seconds,
                    orphan_retention_seconds: DEFAULT_ORPHAN_RETENTION_SECONDS,
                },
            ));
        let retired = match retired {
            Ok(retired) => retired,
            Err(error @ WorkloadReconcileError::Engine { .. }) => {
                let retry = invalidate_engine_connection(
                    &mut self.engine_connection,
                    &mut self.resource_health,
                    now,
                );
                tracing::debug!(
                    attempt = retry.attempt(),
                    retry_milliseconds = retry.duration().as_millis(),
                    error = %error,
                    "disposable garbage collection lost the selected Engine; retry scheduled"
                );

                return;
            }
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "disposable garbage collection blocked");

                return;
            }
        };
        if !retired.is_empty() {
            if let Err(error) = self.control_plane.retire_resources(&retired) {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "disposable state retirement blocked");

                return;
            }
            tracing::info!(
                retired = retired.len(),
                "retired expired orphaned disposable containers"
            );
        }

        let mut workload_resources = Vec::new();
        for application in engine_plan.applications() {
            let result = self.engine_runtime.block_on(reconcile_project_application(
                engine,
                WorkloadReconcileOptions {
                    request: application.request(),
                    installation_id: self.global_network_request.metadata().installation_id(),
                    schema_version: self.global_network_request.metadata().schema_version(),
                },
            ));
            match result {
                Ok(result) => {
                    if let Err(error) = health_snapshot.record(
                        result.container().id().as_str(),
                        result.health(),
                        observed_at_unix_seconds,
                    ) {
                        self.engine_reconciliation.complete();
                        tracing::error!(error = %error, "application health snapshot publication blocked");

                        return;
                    }
                    workload_resources.push(workload_resource_record(&result));
                    tracing::debug!(
                        project = application
                            .request()
                            .metadata()
                            .project_id()
                            .unwrap_or_default(),
                        service = application
                            .request()
                            .metadata()
                            .resource_id()
                            .unwrap_or_default(),
                        action = ?result.action(),
                        "project application reconciliation completed"
                    );
                }
                Err(error @ WorkloadReconcileError::Engine { .. }) => {
                    let retry = invalidate_engine_connection(
                        &mut self.engine_connection,
                        &mut self.resource_health,
                        now,
                    );
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %error,
                        "project application reconciliation lost the selected Engine; retry scheduled"
                    );

                    return;
                }
                Err(error) => {
                    self.engine_reconciliation.complete();
                    tracing::error!(error = %error, "project application reconciliation blocked");

                    return;
                }
            }
        }

        for service in engine_plan.dedicated_services() {
            if let Some(volume) = service.volume() {
                let result = self.engine_runtime.block_on(reconcile_project_volume(
                    engine,
                    ProjectVolumeReconcileOptions {
                        request: volume,
                        installation_id: self.global_network_request.metadata().installation_id(),
                        schema_version: self.global_network_request.metadata().schema_version(),
                    },
                ));
                match result {
                    Ok(result) => {
                        workload_resources.push(project_volume_resource_record(&result));
                        tracing::debug!(
                            project = volume.metadata().project_id().unwrap_or_default(),
                            service = volume.metadata().resource_id().unwrap_or_default(),
                            action = ?result.action(),
                            "dedicated project service volume reconciliation completed"
                        );
                    }
                    Err(error @ WorkloadReconcileError::Engine { .. }) => {
                        let retry = invalidate_engine_connection(
                            &mut self.engine_connection,
                            &mut self.resource_health,
                            now,
                        );
                        tracing::debug!(
                            attempt = retry.attempt(),
                            retry_milliseconds = retry.duration().as_millis(),
                            error = %error,
                            "project volume reconciliation lost the selected Engine; retry scheduled"
                        );

                        return;
                    }
                    Err(error) => {
                        self.engine_reconciliation.complete();
                        tracing::error!(error = %error, "project volume reconciliation blocked");

                        return;
                    }
                }
            }
            let result = self.engine_runtime.block_on(reconcile_project_service(
                engine,
                WorkloadReconcileOptions {
                    request: service.request(),
                    installation_id: self.global_network_request.metadata().installation_id(),
                    schema_version: self.global_network_request.metadata().schema_version(),
                },
            ));
            match result {
                Ok(result) => {
                    if let Err(error) = health_snapshot.record(
                        result.container().id().as_str(),
                        result.health(),
                        observed_at_unix_seconds,
                    ) {
                        self.engine_reconciliation.complete();
                        tracing::error!(error = %error, "project service health snapshot publication blocked");

                        return;
                    }
                    workload_resources.push(workload_resource_record(&result));
                    tracing::debug!(
                        project = service.request().metadata().project_id().unwrap_or_default(),
                        service = service.request().metadata().resource_id().unwrap_or_default(),
                        action = ?result.action(),
                        "dedicated project service reconciliation completed"
                    );
                }
                Err(error @ WorkloadReconcileError::Engine { .. }) => {
                    let retry = invalidate_engine_connection(
                        &mut self.engine_connection,
                        &mut self.resource_health,
                        now,
                    );
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %error,
                        "dedicated project service reconciliation lost the selected Engine; retry scheduled"
                    );

                    return;
                }
                Err(error) => {
                    self.engine_reconciliation.complete();
                    tracing::error!(error = %error, "dedicated project service reconciliation blocked");

                    return;
                }
            }
        }

        for process in engine_plan.processes() {
            let result = self.engine_runtime.block_on(reconcile_project_process(
                engine,
                WorkloadReconcileOptions {
                    request: process,
                    installation_id: self.global_network_request.metadata().installation_id(),
                    schema_version: self.global_network_request.metadata().schema_version(),
                },
            ));
            match result {
                Ok(result) => {
                    if let Err(error) = health_snapshot.record(
                        result.container().id().as_str(),
                        result.health(),
                        observed_at_unix_seconds,
                    ) {
                        self.engine_reconciliation.complete();
                        tracing::error!(error = %error, "process health snapshot publication blocked");

                        return;
                    }
                    workload_resources.push(workload_resource_record(&result));
                    tracing::debug!(
                        project = process.metadata().project_id().unwrap_or_default(),
                        service = process.metadata().resource_id().unwrap_or_default(),
                        action = ?result.action(),
                        "project process reconciliation completed"
                    );
                }
                Err(error @ WorkloadReconcileError::Engine { .. }) => {
                    let retry = invalidate_engine_connection(
                        &mut self.engine_connection,
                        &mut self.resource_health,
                        now,
                    );
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %error,
                        "project process reconciliation lost the selected Engine; retry scheduled"
                    );

                    return;
                }
                Err(error) => {
                    self.engine_reconciliation.complete();
                    tracing::error!(error = %error, "project process reconciliation blocked");

                    return;
                }
            }
        }
        if let Err(error) = self
            .control_plane
            .record_resources(&workload_resources, unix_time_seconds())
        {
            self.engine_reconciliation.complete();
            tracing::error!(error = %error, "project workload ownership publication blocked");

            return;
        }

        let container_user = format!(
            "{}:{}",
            rustix::process::geteuid().as_raw(),
            rustix::process::getegid().as_raw()
        );
        let assets = match prepare_gateway_runtime_assets(GatewayRuntimeAssetOptions {
            runtime_directory: &self.runtime_directory,
            installation_id: self.global_network_request.metadata().installation_id(),
            container_user: &container_user,
            now: time::OffsetDateTime::now_utc(),
        }) {
            Ok(assets) => assets,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "gateway host asset preparation blocked");

                return;
            }
        };
        let mut provider = assets.configuration_provider();
        let gateway_options = match GatewayPlaneOptions::new(
            GatewayReconcileOptions {
                request: assets.request(),
                installation_id: assets.request().metadata().installation_id(),
                schema_version: assets.request().metadata().schema_version(),
                host_probe: &SystemGatewayPortProbe,
            },
            engine_plan.gateway(),
            Duration::from_secs(10),
            Duration::from_millis(100),
        ) {
            Ok(options) => options,
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "gateway plane planning blocked");

                return;
            }
        };
        let gateway = self.engine_runtime.block_on(reconcile_gateway_plane(
            engine,
            &mut provider,
            gateway_options,
        ));

        match gateway {
            Ok(gateway) => {
                self.resource_health = health_snapshot;
                self.engine_reconciliation.complete();
                tracing::debug!(
                    action = ?gateway.gateway_action(),
                    health = ?gateway.health(),
                    configuration_action = ?gateway.configuration_action(),
                    certificate_action = ?assets.certificate_action(),
                    "global gateway reconciliation completed"
                );
            }
            Err(error) => {
                self.engine_reconciliation.complete();
                tracing::error!(error = %error, "global gateway reconciliation blocked");
            }
        }
    }
}

fn validate_options(options: &UnixDaemonRuntimeOptions) -> Result<(), UnixDaemonRuntimeError> {
    if options.idle_poll_interval.is_zero() {
        return Err(invalid(
            "daemon idle poll interval must be greater than zero",
        ));
    }
    if !options.state_database_path.is_absolute()
        || !options.lease_path.is_absolute()
        || !options.socket_path.is_absolute()
    {
        return Err(invalid("daemon runtime paths must be absolute"));
    }
    let socket_parent = options.socket_path.parent();
    if options.state_database_path.parent() != socket_parent
        || options.lease_path.parent() != socket_parent
    {
        return Err(invalid(
            "daemon database, lease, and socket must share one runtime directory",
        ));
    }
    if options.state_database_path == options.lease_path
        || options.state_database_path == options.socket_path
        || options.lease_path == options.socket_path
    {
        return Err(invalid("daemon runtime paths must be distinct"));
    }

    Ok(())
}

fn prepare_runtime_directory(path: &Path) -> Result<(), UnixDaemonRuntimeError> {
    std::fs::create_dir_all(path).map_err(|source| file_system_error("create", path, source))?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|source| file_system_error("inspect", path, source))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid(
            "daemon runtime directory must be a real directory, not a link",
        ));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|source| file_system_error("secure", path, source))
}

fn remove_stale_socket(path: &Path) -> Result<(), UnixDaemonRuntimeError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(path)
            .map_err(|source| file_system_error("remove stale socket", path, source)),
        Ok(_) => Err(invalid(
            "daemon socket path exists but is not an owned Unix socket",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(file_system_error("inspect socket", path, source)),
    }
}

pub(super) fn runtime_linux_platform() -> Result<&'static str, String> {
    match std::env::consts::ARCH {
        "aarch64" => Ok("linux/arm64"),
        "x86_64" => Ok("linux/amd64"),
        architecture => Err(format!(
            "host architecture '{architecture}' has no supported Linux Engine platform mapping"
        )),
    }
}

pub(super) fn merge_prepared_environments(
    execution: &crate::control_plane::ExecutionPlan,
    prepared: &[PreparedSharedInstance],
) -> Result<Vec<ManagedEnvironmentRecord>, String> {
    let mut generated = execution
        .services()
        .iter()
        .map(|service| (service.project().as_str().to_owned(), BTreeMap::new()))
        .collect::<BTreeMap<String, BTreeMap<String, String>>>();

    for environment in prepared
        .iter()
        .flat_map(PreparedSharedInstance::environments)
    {
        let project_id = environment.project_id();
        let values = generated.entry(project_id.to_owned()).or_default();
        for (key, value) in environment.values() {
            if values.get(key).is_some_and(|existing| existing != value) {
                return Err(format!(
                    "project '{project_id}' has conflicting generated environment key '{key}'"
                ));
            }
            values.insert(key.clone(), value.clone());
        }
    }

    generated
        .into_iter()
        .map(|(project_id, values)| {
            let canonical = serde_json::to_vec(&values)
                .map_err(|error| format!("failed to encode managed environment: {error}"))?;
            Ok(ManagedEnvironmentRecord::new(
                ManagedEnvironmentRecordOptions {
                    project_id,
                    revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
                    values,
                    lifecycle: EnvironmentLifecycle::Active,
                },
            ))
        })
        .collect()
}

fn unix_time_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(i64::MAX)
}

fn invalid(detail: impl Into<String>) -> UnixDaemonRuntimeError {
    UnixDaemonRuntimeError::InvalidOptions {
        detail: detail.into(),
    }
}

fn file_system_error(
    action: &'static str,
    path: &Path,
    source: std::io::Error,
) -> UnixDaemonRuntimeError {
    UnixDaemonRuntimeError::FileSystem {
        action,
        path: path.to_path_buf(),
        source,
    }
}
