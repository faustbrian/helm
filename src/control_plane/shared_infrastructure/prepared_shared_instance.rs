use super::{
    PreparedGotenbergSharedInstance, PreparedMailpitSharedInstance, PreparedMongoDbSharedInstance,
    PreparedMySqlSharedInstance, PreparedObjectStoreSharedInstance, PreparedPostgresSharedInstance,
    PreparedRabbitMqSharedInstance, PreparedRedisSharedInstance, PreparedSqlServerSharedInstance,
};
use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::state::ManagedEnvironmentRecord;

/// Backend-specific prepared state hidden behind one daemon strategy boundary.
pub(crate) enum PreparedSharedInstance {
    Postgres(PreparedPostgresSharedInstance),
    MySql(PreparedMySqlSharedInstance),
    Redis(PreparedRedisSharedInstance),
    ObjectStore(PreparedObjectStoreSharedInstance),
    RabbitMq(PreparedRabbitMqSharedInstance),
    Mailpit(PreparedMailpitSharedInstance),
    MongoDb(PreparedMongoDbSharedInstance),
    SqlServer(PreparedSqlServerSharedInstance),
    Gotenberg(PreparedGotenbergSharedInstance),
}

impl PreparedSharedInstance {
    pub(crate) fn container_request(&self) -> &ContainerCreateOptions {
        match self {
            Self::Postgres(prepared) => prepared.instance().container(),
            Self::MySql(prepared) => prepared.instance().container(),
            Self::Redis(prepared) => prepared.instance().container(),
            Self::ObjectStore(prepared) => prepared.instance().container(),
            Self::RabbitMq(prepared) => prepared.instance().container(),
            Self::Mailpit(prepared) => prepared.instance().container(),
            Self::MongoDb(prepared) => prepared.instance().container(),
            Self::SqlServer(prepared) => prepared.instance().container(),
            Self::Gotenberg(prepared) => prepared.instance().container(),
        }
    }

    pub(crate) fn credential_service_identities(&self) -> Vec<(String, String)> {
        match self {
            Self::Gotenberg(_) => Vec::new(),
            _ => self.service_identities(),
        }
    }

    pub(crate) fn service_environments(&self) -> Vec<(String, String, &ManagedEnvironmentRecord)> {
        self.service_identities()
            .into_iter()
            .zip(self.environments())
            .map(|((project_id, service_id), environment)| (project_id, service_id, environment))
            .collect()
    }

    pub(crate) fn service_identities(&self) -> Vec<(String, String)> {
        match self {
            Self::Postgres(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.logical().project_id().to_owned(),
                        project.logical().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::MySql(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.environment().project_id().to_owned(),
                        project.credential().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::Redis(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.environment().project_id().to_owned(),
                        project.credential().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::ObjectStore(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.environment().project_id().to_owned(),
                        project.credential().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::RabbitMq(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.environment().project_id().to_owned(),
                        project.credential().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::Mailpit(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.environment().project_id().to_owned(),
                        project.credential().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::MongoDb(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.environment().project_id().to_owned(),
                        project.credential().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::SqlServer(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.environment().project_id().to_owned(),
                        project.credential().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::Gotenberg(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project.project_id().to_owned(),
                        project.service_id().to_owned(),
                    )
                })
                .collect(),
        }
    }

    pub(crate) fn environments(&self) -> Vec<&ManagedEnvironmentRecord> {
        match self {
            Self::Postgres(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
            Self::MySql(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
            Self::Redis(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
            Self::ObjectStore(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
            Self::RabbitMq(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
            Self::Mailpit(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
            Self::MongoDb(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
            Self::SqlServer(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
            Self::Gotenberg(prepared) => prepared
                .projects()
                .iter()
                .map(|project| project.environment())
                .collect(),
        }
    }

    pub(crate) fn routes(&self) -> Vec<GatewayRoute> {
        match self {
            Self::Mailpit(prepared) => prepared.routes(),
            _ => Vec::new(),
        }
    }
}
