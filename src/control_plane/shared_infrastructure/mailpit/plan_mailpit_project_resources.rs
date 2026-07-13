use super::{
    MailpitPlanError, MailpitProjectDefinition, MailpitProjectResources, MailpitSharedInstancePlan,
};
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use crate::control_plane::{ProjectIdentity, RouteIdentity, ServiceIdentity};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// Composes one attributed SMTP identity, environment, credential, and UI route.
pub(crate) fn plan_mailpit_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &MailpitSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<MailpitProjectResources, MailpitPlanError> {
    let definition = MailpitProjectDefinition::new(project_id, service_id, secret.clone())?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/mailpit"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: definition.username().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        (
            "MAIL_HOST".to_owned(),
            instance.container().name().to_owned(),
        ),
        ("MAIL_MAILER".to_owned(), "smtp".to_owned()),
        ("MAIL_PASSWORD".to_owned(), secret.expose().to_owned()),
        ("MAIL_PORT".to_owned(), instance.smtp_port().to_string()),
        ("MAIL_USERNAME".to_owned(), definition.username().to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        MailpitPlanError::new(format!(
            "failed to encode Mailpit managed environment: {error}"
        ))
    })?;
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(canonical)));
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision,
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });
    let project = ProjectIdentity::resolve(Some(project_id), Path::new("/"))
        .map_err(|error| MailpitPlanError::new(error.to_string()))?;
    let service = ServiceIdentity::new(service_id)
        .map_err(|error| MailpitPlanError::new(error.to_string()))?;
    let route = RouteIdentity::new(&project, &service)
        .map_err(|error| MailpitPlanError::new(error.to_string()))?;
    let upstream = format!(
        "http://{}:{}",
        instance.container().name(),
        instance.ui_port()
    );
    let route = GatewayRoute::new(route.domain(), upstream)
        .map_err(|error| MailpitPlanError::new(error.to_string()))?;

    Ok(MailpitProjectResources::new(
        definition,
        credential,
        environment,
        route,
    ))
}
