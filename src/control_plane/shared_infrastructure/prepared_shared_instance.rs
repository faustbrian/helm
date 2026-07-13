use super::{
    PreparedMySqlSharedInstance, PreparedPostgresSharedInstance, PreparedRedisSharedInstance,
};
use crate::control_plane::state::ManagedEnvironmentRecord;

/// Backend-specific prepared state hidden behind one daemon strategy boundary.
pub(crate) enum PreparedSharedInstance {
    Postgres(PreparedPostgresSharedInstance),
    MySql(PreparedMySqlSharedInstance),
    Redis(PreparedRedisSharedInstance),
}

impl PreparedSharedInstance {
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
                        project
                            .credential()
                            .project_id()
                            .expect("project MySQL credential owner")
                            .to_owned(),
                        project.credential().service_id().to_owned(),
                    )
                })
                .collect(),
            Self::Redis(prepared) => prepared
                .projects()
                .iter()
                .map(|project| {
                    (
                        project
                            .credential()
                            .project_id()
                            .expect("project Redis credential owner")
                            .to_owned(),
                        project.credential().service_id().to_owned(),
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
        }
    }
}
