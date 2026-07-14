use super::{MySqlFlavor, MySqlPlanError};
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use std::fmt::{Debug, Formatter};

const SCHEMA_IDENTIFIER_BYTES: usize = 64;
const USER_IDENTIFIER_BYTES: usize = 32;

/// Idempotent schema and restricted-user provisioning input.
pub(crate) struct MySqlLogicalResourcePlan {
    flavor: MySqlFlavor,
    schema_name: String,
    username: String,
    credential_id: String,
    stdin_sql: String,
}

impl MySqlLogicalResourcePlan {
    pub(crate) fn new(
        flavor: MySqlFlavor,
        project_id: &str,
        service_id: &str,
        secret: CredentialSecret,
    ) -> Result<Self, MySqlPlanError> {
        let project_id = DnsLabel::new("project", project_id)
            .map_err(|error| MySqlPlanError::new(error.to_string()))?;
        let service_id = DnsLabel::new("service", service_id)
            .map_err(|error| MySqlPlanError::new(error.to_string()))?;
        let project_part = sql_identifier_part(project_id.as_str());
        let service_part = sql_identifier_part(service_id.as_str());
        let schema_name = format!("stackctl_{project_part}_{service_part}");
        let username = format!("st_{project_part}_{service_part}");
        validate_length("schema", &schema_name, SCHEMA_IDENTIFIER_BYTES)?;
        validate_length("user", &username, USER_IDENTIFIER_BYTES)?;
        let credential_id = format!(
            "{}/{}/{}",
            project_id.as_str(),
            service_id.as_str(),
            flavor.implementation()
        );
        let stdin_sql = provisioning_sql(&schema_name, &username, secret.expose());

        Ok(Self {
            flavor,
            schema_name,
            username,
            credential_id,
            stdin_sql,
        })
    }

    pub(crate) const fn flavor(&self) -> MySqlFlavor {
        self.flavor
    }

    pub(crate) fn schema_name(&self) -> &str {
        &self.schema_name
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn credential_id(&self) -> &str {
        &self.credential_id
    }

    pub(crate) fn stdin_sql(&self) -> &str {
        &self.stdin_sql
    }
}

impl Debug for MySqlLogicalResourcePlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MySqlLogicalResourcePlan")
            .field("flavor", &self.flavor)
            .field("schema_name", &self.schema_name)
            .field("username", &self.username)
            .field("credential_id", &self.credential_id)
            .field("stdin_sql", &"[REDACTED]")
            .finish()
    }
}

fn sql_identifier_part(value: &str) -> String {
    value.replace('-', "_")
}

fn validate_length(kind: &str, identifier: &str, maximum: usize) -> Result<(), MySqlPlanError> {
    if identifier.len() <= maximum {
        return Ok(());
    }

    Err(MySqlPlanError::new(format!(
        "MySQL {kind} identifier '{identifier}' exceeds the {maximum}-byte compatibility limit"
    )))
}

fn provisioning_sql(schema_name: &str, username: &str, secret: &str) -> String {
    let secret = secret.replace('\'', "''");

    format!(
        "CREATE DATABASE IF NOT EXISTS `{schema_name}`;\n\
         CREATE USER IF NOT EXISTS '{username}'@'%' IDENTIFIED BY '{secret}';\n\
         ALTER USER '{username}'@'%' IDENTIFIED BY '{secret}';\n\
         GRANT ALL PRIVILEGES ON `{schema_name}`.* TO '{username}'@'%';\n"
    )
}
