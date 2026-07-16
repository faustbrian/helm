use super::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerCreateOptions, ContainerDiscovery, ContainerHealth, ContainerId, ContainerLifecycle,
    ContainerNetworkIsolation, ContainerState, EngineFuture, HealthObserver, ImageId,
    ImageResolver, ImmutableImageReference, NetworkDiscovery, ObservedContainer, ObservedNetwork,
    ObservedVolume, OwnedContainer, OwnedNetwork, OwnedVolume, VolumeCreateOptions,
    VolumeDiscovery, VolumeManager,
};

/// Delegates Engine mutations while serving one reconciliation-pass observation.
pub(crate) struct ReconciliationEngine<'observation, Engine> {
    engine: Engine,
    containers: &'observation [ObservedContainer],
    volumes: &'observation [ObservedVolume],
    networks: &'observation [ObservedNetwork],
}

impl<'observation, Engine> ReconciliationEngine<'observation, Engine> {
    pub(crate) const fn new(
        engine: Engine,
        containers: &'observation [ObservedContainer],
        volumes: &'observation [ObservedVolume],
        networks: &'observation [ObservedNetwork],
    ) -> Self {
        Self {
            engine,
            containers,
            volumes,
            networks,
        }
    }
}

impl<Engine> ContainerDiscovery for ReconciliationEngine<'_, Engine> {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        let containers = self.containers.to_vec();

        Box::pin(async move { Ok(containers) })
    }
}

impl<Engine> VolumeDiscovery for ReconciliationEngine<'_, Engine> {
    fn discover_managed_volumes(&self) -> EngineFuture<'_, Vec<ObservedVolume>> {
        let volumes = self.volumes.to_vec();

        Box::pin(async move { Ok(volumes) })
    }
}

impl<Engine> NetworkDiscovery for ReconciliationEngine<'_, Engine> {
    fn discover_managed_networks(&self) -> EngineFuture<'_, Vec<ObservedNetwork>> {
        let networks = self.networks.to_vec();

        Box::pin(async move { Ok(networks) })
    }
}

impl<Engine: CommandExecutor> CommandExecutor for ReconciliationEngine<'_, Engine> {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.engine.start_command(container, request)
    }

    fn command_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.engine.command_status(execution_id, container_id)
    }
}

impl<Engine: ContainerLifecycle> ContainerLifecycle for ReconciliationEngine<'_, Engine> {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        self.engine.create(options)
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        self.engine.start(container)
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        self.engine.stop(container)
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        self.engine.remove(container)
    }

    fn inspect<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState> {
        self.engine.inspect(container)
    }
}

impl<Engine: ContainerNetworkIsolation> ContainerNetworkIsolation
    for ReconciliationEngine<'_, Engine>
{
    fn disconnect_container_network<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        self.engine.disconnect_container_network(container, network)
    }

    fn reconnect_container_network<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        network: &'operation OwnedNetwork,
        alias: &'operation str,
    ) -> EngineFuture<'operation, ()> {
        self.engine
            .reconnect_container_network(container, network, alias)
    }
}

impl<Engine: HealthObserver> HealthObserver for ReconciliationEngine<'_, Engine> {
    fn observe_health<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerHealth> {
        self.engine.observe_health(container)
    }
}

impl<Engine: ImageResolver> ImageResolver for ReconciliationEngine<'_, Engine> {
    fn ensure_image<'operation>(
        &'operation mut self,
        reference: &'operation ImmutableImageReference,
    ) -> EngineFuture<'operation, ImageId> {
        self.engine.ensure_image(reference)
    }
}

impl<Engine: VolumeManager> VolumeManager for ReconciliationEngine<'_, Engine> {
    fn create_volume<'operation>(
        &'operation mut self,
        options: &'operation VolumeCreateOptions,
    ) -> EngineFuture<'operation, OwnedVolume> {
        self.engine.create_volume(options)
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        volume: &'operation OwnedVolume,
    ) -> EngineFuture<'operation, ()> {
        self.engine.remove_volume(volume)
    }
}
