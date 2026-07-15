use super::project_service_http_readiness_options::{
    ProjectServiceHttpAuthentication, ProjectServiceHttpReadinessOptions,
};
use super::{ProjectServicePreparationError, ProjectServiceProvisioningJob};
use crate::control_plane::project_infrastructure::project_service_provisioning_images::CURL_CLIENT_IMAGE;
use std::collections::BTreeMap;

/// Builds one secret-safe authenticated HTTP readiness job.
pub(super) fn project_service_http_readiness_job(
    options: ProjectServiceHttpReadinessOptions<'_>,
) -> Result<ProjectServiceProvisioningJob, ProjectServicePreparationError> {
    let mut command = vec![
        "--fail-with-body".to_owned(),
        "--silent".to_owned(),
        "--show-error".to_owned(),
        "--max-time".to_owned(),
        "5".to_owned(),
        "--output".to_owned(),
        "/dev/null".to_owned(),
    ];
    if options.allow_invalid_certificate {
        command.push("--insecure".to_owned());
    }
    let mut environment = BTreeMap::new();
    match options.authentication {
        ProjectServiceHttpAuthentication::Basic {
            username,
            environment_key,
            secret,
        } => {
            import_environment(&mut command, environment_key);
            command.extend([
                "--expand-user".to_owned(),
                format!("{username}:{{{{{environment_key}}}}}"),
            ]);
            environment.insert(environment_key.to_owned(), secret.to_owned());
        }
        ProjectServiceHttpAuthentication::Bearer {
            environment_key,
            secret,
        } => {
            import_environment(&mut command, environment_key);
            command.extend([
                "--expand-header".to_owned(),
                format!("Authorization: Bearer {{{{{environment_key}}}}}"),
            ]);
            environment.insert(environment_key.to_owned(), secret.to_owned());
        }
        ProjectServiceHttpAuthentication::Header {
            header_name,
            environment_key,
            secret,
        } => {
            import_environment(&mut command, environment_key);
            command.extend([
                "--expand-header".to_owned(),
                format!("{header_name}: {{{{{environment_key}}}}}"),
            ]);
            environment.insert(environment_key.to_owned(), secret.to_owned());
        }
    }
    command.push(options.url);

    ProjectServiceProvisioningJob::new(CURL_CLIENT_IMAGE, command, environment)
}

fn import_environment(command: &mut Vec<String>, environment_key: &str) {
    command.extend(["--variable".to_owned(), format!("%{environment_key}")]);
}
