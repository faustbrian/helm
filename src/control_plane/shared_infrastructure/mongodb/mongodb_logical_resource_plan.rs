use super::MongoDbPlanError;
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::CredentialRecord;
use std::fmt::{Debug, Formatter};

/// Idempotent MongoDB database-user provisioning input.
pub(crate) struct MongoDbLogicalResourcePlan {
    database_name: String,
    username: String,
    credential_id: String,
    stdin_script: String,
}

impl MongoDbLogicalResourcePlan {
    pub(crate) fn new(
        project_id: &str,
        service_id: &str,
        secret: CredentialSecret,
        bootstrap_secret: CredentialSecret,
    ) -> Result<Self, MongoDbPlanError> {
        let project_id = DnsLabel::new("project", project_id)
            .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
        let service_id = DnsLabel::new("service", service_id)
            .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
        let identity = format!(
            "{}_{}",
            project_id.as_str().replace('-', "_"),
            service_id.as_str().replace('-', "_")
        );
        let database_name = format!("stackctl_{identity}");
        let username = format!("st_{identity}");
        let credential_id = format!("{}/{}/mongodb", project_id.as_str(), service_id.as_str());
        let stdin_script = provisioning_script(
            &database_name,
            &username,
            secret.expose(),
            bootstrap_secret.expose(),
        )?;

        Ok(Self {
            database_name,
            username,
            credential_id,
            stdin_script,
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

    pub(crate) fn matches_credential(
        &self,
        credential: &CredentialRecord,
        bootstrap_secret: &str,
    ) -> bool {
        credential.credential_id() == self.credential_id()
            && credential.username() == self.username()
            && self.stdin_script
                == provisioning_script(
                    self.database_name(),
                    self.username(),
                    credential.secret(),
                    bootstrap_secret,
                )
                .unwrap_or_default()
    }

    pub(crate) fn stdin_script(&self) -> &str {
        &self.stdin_script
    }

    pub(crate) fn command_arguments(&self) -> [&'static str; 3] {
        ["mongosh", "--quiet", "--nodb"]
    }
}

impl Debug for MongoDbLogicalResourcePlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MongoDbLogicalResourcePlan")
            .field("database_name", &self.database_name)
            .field("username", &self.username)
            .field("credential_id", &self.credential_id)
            .field("stdin_script", &"[REDACTED]")
            .finish()
    }
}

fn provisioning_script(
    database: &str,
    username: &str,
    secret: &str,
    bootstrap_secret: &str,
) -> Result<String, MongoDbPlanError> {
    let database = json_string(database)?;
    let username = json_string(username)?;
    let secret = json_string(secret)?;
    let bootstrap_secret = json_string(bootstrap_secret)?;

    Ok(format!(
        "try {{\n\
         const admin = connect(\"mongodb://127.0.0.1:27017/admin\");\n\
         if (!admin.auth(\"stackctl_admin\", {bootstrap_secret})) {{ throw new Error(\"admin authentication failed\"); }}\n\
         const target = admin.getSiblingDB({database});\n\
         const roles = [{{ role: \"readWrite\", db: {database} }}];\n\
         if (target.getUser({username}) === null) {{\n\
           target.createUser({{ user: {username}, pwd: {secret}, roles }});\n\
         }} else {{\n\
           target.updateUser({username}, {{ pwd: {secret}, roles }});\n\
         }}\n\
         }} catch (error) {{ print(error); quit(1); }}\n"
    ))
}

fn json_string(value: &str) -> Result<String, MongoDbPlanError> {
    serde_json::to_string(value)
        .map_err(|error| MongoDbPlanError::new(format!("failed to encode MongoDB script: {error}")))
}
