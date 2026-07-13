use super::{
    PreparedSharedInstance, SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
    reconcile_prepared_mailpit_instance, reconcile_prepared_mysql_instance,
    reconcile_prepared_object_store_instance, reconcile_prepared_postgres_instance,
    reconcile_prepared_rabbitmq_instance, reconcile_prepared_redis_instance,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};

/// Dispatches a prepared instance through its backend-specific convergence strategy.
pub(crate) async fn reconcile_prepared_shared_instance<Engine>(
    engine: &mut Engine,
    prepared: &PreparedSharedInstance,
    installation_id: &str,
    schema_version: u32,
) -> Result<SharedInstanceReconcileResult, SharedInfrastructureReconcileError>
where
    Engine: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager,
{
    match prepared {
        PreparedSharedInstance::Postgres(prepared) => {
            reconcile_prepared_postgres_instance(engine, prepared, installation_id, schema_version)
                .await
        }
        PreparedSharedInstance::MySql(prepared) => {
            reconcile_prepared_mysql_instance(engine, prepared, installation_id, schema_version)
                .await
        }
        PreparedSharedInstance::Redis(prepared) => {
            reconcile_prepared_redis_instance(engine, prepared, installation_id, schema_version)
                .await
        }
        PreparedSharedInstance::ObjectStore(prepared) => {
            reconcile_prepared_object_store_instance(
                engine,
                prepared,
                installation_id,
                schema_version,
            )
            .await
        }
        PreparedSharedInstance::RabbitMq(prepared) => {
            reconcile_prepared_rabbitmq_instance(engine, prepared, installation_id, schema_version)
                .await
        }
        PreparedSharedInstance::Mailpit(prepared) => {
            reconcile_prepared_mailpit_instance(engine, prepared, installation_id, schema_version)
                .await
        }
    }
}
