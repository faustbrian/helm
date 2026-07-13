use super::PostgresPlanError;
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use std::fmt::{Debug, Formatter};

const POSTGRES_IDENTIFIER_BYTES: usize = 63;

/// Idempotent PostgreSQL database and restricted-role provisioning input.
pub(crate) struct PostgresLogicalResourcePlan {
    database_name: String,
    role_name: String,
    credential_id: String,
    command_arguments: Vec<String>,
    stdin_sql: String,
}

impl PostgresLogicalResourcePlan {
    pub(crate) fn new(
        project_id: &str,
        service_id: &str,
        secret: CredentialSecret,
    ) -> Result<Self, PostgresPlanError> {
        let project_id = DnsLabel::new("project", project_id)
            .map_err(|error| PostgresPlanError::new(error.to_string()))?;
        let service_id = DnsLabel::new("service", service_id)
            .map_err(|error| PostgresPlanError::new(error.to_string()))?;
        let database_name = format!(
            "stackctl_{}_{}",
            sql_identifier_part(project_id.as_str()),
            sql_identifier_part(service_id.as_str())
        );
        let role_name = format!("{database_name}_role");
        validate_identifier_length("database", &database_name)?;
        validate_identifier_length("role", &role_name)?;
        let credential_id = format!("{}/{}/postgresql", project_id.as_str(), service_id.as_str());
        let stdin_sql = provisioning_sql(&database_name, &role_name, secret.expose());

        Ok(Self {
            database_name,
            role_name,
            credential_id,
            command_arguments: vec![
                "psql".to_owned(),
                "--no-psqlrc".to_owned(),
                "--set=ON_ERROR_STOP=1".to_owned(),
                "--username=postgres".to_owned(),
                "--dbname=postgres".to_owned(),
            ],
            stdin_sql,
        })
    }

    pub(crate) fn database_name(&self) -> &str {
        &self.database_name
    }

    pub(crate) fn role_name(&self) -> &str {
        &self.role_name
    }

    pub(crate) fn credential_id(&self) -> &str {
        &self.credential_id
    }

    pub(crate) fn command_arguments(&self) -> &[String] {
        &self.command_arguments
    }

    pub(crate) fn stdin_sql(&self) -> &str {
        &self.stdin_sql
    }
}

impl Debug for PostgresLogicalResourcePlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresLogicalResourcePlan")
            .field("database_name", &self.database_name)
            .field("role_name", &self.role_name)
            .field("credential_id", &self.credential_id)
            .field("command_arguments", &self.command_arguments)
            .field("stdin_sql", &"[REDACTED]")
            .finish()
    }
}

fn sql_identifier_part(value: &str) -> String {
    value.replace('-', "_")
}

fn validate_identifier_length(kind: &str, identifier: &str) -> Result<(), PostgresPlanError> {
    if identifier.len() <= POSTGRES_IDENTIFIER_BYTES {
        return Ok(());
    }

    Err(PostgresPlanError::new(format!(
        "PostgreSQL {kind} identifier '{identifier}' exceeds PostgreSQL's {POSTGRES_IDENTIFIER_BYTES}-byte limit"
    )))
}

fn provisioning_sql(database_name: &str, role_name: &str, secret: &str) -> String {
    let secret = sql_string_literal(secret);

    format!(
        "DO $stackctl$\n\
         BEGIN\n\
             IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{role_name}') THEN\n\
                 CREATE ROLE {role_name} LOGIN PASSWORD '{secret}';\n\
             ELSE\n\
                 ALTER ROLE {role_name} LOGIN PASSWORD '{secret}';\n\
             END IF;\n\
         END\n\
         $stackctl$;\n\
         SELECT 'CREATE DATABASE {database_name} OWNER {role_name}'\n\
         WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = '{database_name}')\\gexec\n\
         ALTER DATABASE {database_name} OWNER TO {role_name};\n\
         REVOKE ALL ON DATABASE {database_name} FROM PUBLIC;\n\
         GRANT CONNECT, TEMPORARY ON DATABASE {database_name} TO {role_name};\n"
    )
}

fn sql_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}
