use super::SqlServerPlanError;
use super::sql_server_shared_instance_plan::validate_password;
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use std::fmt::{Debug, Formatter};

const IDENTIFIER_BYTES: usize = 128;

/// Idempotent SQL Server database, login, user, and role provisioning input.
pub(crate) struct SqlServerLogicalResourcePlan {
    database_name: String,
    username: String,
    credential_id: String,
    stdin_sql: String,
}

impl SqlServerLogicalResourcePlan {
    pub(crate) fn new(
        project_id: &str,
        service_id: &str,
        secret: CredentialSecret,
    ) -> Result<Self, SqlServerPlanError> {
        let project_id = DnsLabel::new("project", project_id)
            .map_err(|error| SqlServerPlanError::new(error.to_string()))?;
        let service_id = DnsLabel::new("service", service_id)
            .map_err(|error| SqlServerPlanError::new(error.to_string()))?;
        validate_password(secret.expose(), "project")?;
        let identity = format!(
            "{}_{}",
            project_id.as_str().replace('-', "_"),
            service_id.as_str().replace('-', "_")
        );
        let database_name = format!("stackctl_{identity}");
        let username = format!("st_{identity}");
        for (kind, value) in [("database", &database_name), ("login", &username)] {
            if value.len() > IDENTIFIER_BYTES {
                return Err(SqlServerPlanError::new(format!(
                    "SQL Server {kind} identifier '{value}' exceeds {IDENTIFIER_BYTES} bytes"
                )));
            }
        }
        let credential_id = format!("{}/{}/sqlserver", project_id.as_str(), service_id.as_str());
        let stdin_sql = provisioning_sql(&database_name, &username, secret.expose());

        Ok(Self {
            database_name,
            username,
            credential_id,
            stdin_sql,
        })
    }

    pub(crate) fn database_name(&self) -> &str {
        &self.database_name
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

impl Debug for SqlServerLogicalResourcePlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqlServerLogicalResourcePlan")
            .field("database_name", &self.database_name)
            .field("username", &self.username)
            .field("credential_id", &self.credential_id)
            .field("stdin_sql", &"[REDACTED]")
            .finish()
    }
}

fn provisioning_sql(database: &str, username: &str, secret: &str) -> String {
    let secret = secret.replace('\'', "''");

    format!(
        "IF DB_ID(N'{database}') IS NULL CREATE DATABASE [{database}];\n\
         IF NOT EXISTS (SELECT 1 FROM sys.server_principals WHERE name = N'{username}')\n\
             CREATE LOGIN [{username}] WITH PASSWORD = N'{secret}', CHECK_POLICY = OFF;\n\
         ELSE ALTER LOGIN [{username}] WITH PASSWORD = N'{secret}';\n\
         ALTER LOGIN [{username}] ENABLE;\n\
         GO\n\
         USE [{database}];\n\
         IF NOT EXISTS (SELECT 1 FROM sys.database_principals WHERE name = N'{username}')\n\
             CREATE USER [{username}] FOR LOGIN [{username}];\n\
         IF IS_ROLEMEMBER(N'db_owner', N'{username}') <> 1\n\
             ALTER ROLE [db_owner] ADD MEMBER [{username}];\n"
    )
}
