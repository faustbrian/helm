use super::project_service_http_readiness_options::{
    ProjectServiceHttpAuthentication, ProjectServiceHttpReadinessOptions,
};
use super::{ProjectServicePreparationError, ProjectServiceProvisioningJob};
use crate::control_plane::project_infrastructure::project_service_provisioning_images::CURL_CLIENT_IMAGE;
use std::collections::BTreeMap;

const AUTHENTICATION_FAILURE_EXIT_STATUS: i64 = 42;

/// Builds one secret-safe authenticated HTTP readiness job.
pub(super) fn project_service_http_readiness_job(
    options: ProjectServiceHttpReadinessOptions<'_>,
) -> Result<ProjectServiceProvisioningJob, ProjectServicePreparationError> {
    let mut curl = vec![
        "--silent".to_owned(),
        "--show-error".to_owned(),
        "--max-time".to_owned(),
        "5".to_owned(),
        "--output".to_owned(),
        "/dev/null".to_owned(),
    ];
    if options.allow_invalid_certificate {
        curl.push("--insecure".to_owned());
    }
    let mut environment = BTreeMap::new();
    match options.authentication {
        ProjectServiceHttpAuthentication::Basic {
            username,
            environment_key,
            secret,
        } => {
            import_environment(&mut curl, environment_key);
            curl.extend([
                "--expand-user".to_owned(),
                format!("{username}:{{{{{environment_key}}}}}"),
            ]);
            environment.insert(environment_key.to_owned(), secret.to_owned());
        }
        ProjectServiceHttpAuthentication::Bearer {
            environment_key,
            secret,
        } => {
            import_environment(&mut curl, environment_key);
            curl.extend([
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
            import_environment(&mut curl, environment_key);
            curl.extend([
                "--expand-header".to_owned(),
                format!("{header_name}: {{{{{environment_key}}}}}"),
            ]);
            environment.insert(environment_key.to_owned(), secret.to_owned());
        }
    }
    curl.extend([
        "--write-out".to_owned(),
        "%{http_code}".to_owned(),
        options.url,
    ]);
    let curl = curl
        .iter()
        .map(|argument| shell_word(argument))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "status=\"$(curl {curl})\" || exit 1\ncase \"$status\" in 401|403) exit {AUTHENTICATION_FAILURE_EXIT_STATUS} ;; 2??) exit 0 ;; *) exit 1 ;; esac"
    );

    ProjectServiceProvisioningJob::new(
        CURL_CLIENT_IMAGE,
        vec!["sh".to_owned(), "-ec".to_owned(), script],
        environment,
    )?
    .with_authentication_failure_exit_status(AUTHENTICATION_FAILURE_EXIT_STATUS)
}

fn import_environment(command: &mut Vec<String>, environment_key: &str) {
    command.extend(["--variable".to_owned(), format!("%{environment_key}")]);
}

fn shell_word(argument: &str) -> String {
    format!("'{}'", argument.replace("'", "'\"'\"'"))
}
