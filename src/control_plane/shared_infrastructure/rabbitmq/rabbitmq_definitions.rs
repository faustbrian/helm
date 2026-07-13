use super::{RabbitMqPlanError, RabbitMqProjectDefinition};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

const HASHING_ALGORITHM: &str = "rabbit_password_hashing_sha256";

/// Complete deterministic core definitions for all projects sharing a broker.
pub(crate) struct RabbitMqDefinitions {
    contents: Vec<u8>,
    project_count: usize,
}

impl RabbitMqDefinitions {
    pub(crate) fn new(projects: Vec<RabbitMqProjectDefinition>) -> Result<Self, RabbitMqPlanError> {
        let mut by_username = BTreeMap::new();
        for project in projects {
            let username = project.username().to_owned();
            if by_username.insert(username.clone(), project).is_some() {
                return Err(RabbitMqPlanError::new(format!(
                    "RabbitMQ user '{username}' is defined more than once"
                )));
            }
        }
        let document = DefinitionsDocument {
            users: by_username
                .values()
                .map(|project| UserDefinition {
                    name: project.username(),
                    password_hash: project.password_hash().encoded(),
                    hashing_algorithm: HASHING_ALGORITHM,
                    tags: Vec::new(),
                })
                .collect(),
            vhosts: by_username
                .values()
                .map(|project| VhostDefinition {
                    name: project.vhost(),
                })
                .collect(),
            permissions: by_username
                .values()
                .map(|project| PermissionDefinition {
                    user: project.username(),
                    vhost: project.vhost(),
                    configure: ".*",
                    write: ".*",
                    read: ".*",
                })
                .collect(),
            topic_permissions: Vec::new(),
            parameters: Vec::new(),
            global_parameters: Vec::new(),
            policies: Vec::new(),
            queues: Vec::new(),
            exchanges: Vec::new(),
            bindings: Vec::new(),
        };
        let contents = serde_json::to_vec(&document).map_err(|error| {
            RabbitMqPlanError::new(format!("failed to encode RabbitMQ definitions: {error}"))
        })?;

        Ok(Self {
            contents,
            project_count: by_username.len(),
        })
    }

    pub(crate) fn contents(&self) -> &[u8] {
        &self.contents
    }
}

impl Debug for RabbitMqDefinitions {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RabbitMqDefinitions")
            .field("project_count", &self.project_count)
            .finish()
    }
}

#[derive(Serialize)]
struct DefinitionsDocument<'definition> {
    users: Vec<UserDefinition<'definition>>,
    vhosts: Vec<VhostDefinition<'definition>>,
    permissions: Vec<PermissionDefinition<'definition>>,
    topic_permissions: Vec<serde_json::Value>,
    parameters: Vec<serde_json::Value>,
    global_parameters: Vec<serde_json::Value>,
    policies: Vec<serde_json::Value>,
    queues: Vec<serde_json::Value>,
    exchanges: Vec<serde_json::Value>,
    bindings: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct UserDefinition<'definition> {
    name: &'definition str,
    password_hash: &'definition str,
    hashing_algorithm: &'static str,
    tags: Vec<&'static str>,
}

#[derive(Serialize)]
struct VhostDefinition<'definition> {
    name: &'definition str,
}

#[derive(Serialize)]
struct PermissionDefinition<'definition> {
    user: &'definition str,
    vhost: &'definition str,
    configure: &'static str,
    write: &'static str,
    read: &'static str,
}
