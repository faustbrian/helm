use super::MySqlLogicalPruneOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, run_attached_command,
};
use crate::control_plane::shared_infrastructure::MySqlFlavor;
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

/// Idempotently deletes one exact orphaned schema and user from a shared instance.
pub(crate) async fn prune_mysql_logical_resource(
    executor: &impl CommandExecutor,
    options: MySqlLogicalPruneOptions<'_>,
) -> Result<(), EngineError> {
    validate(&options)?;
    let request = CommandRequest::new(
        vec![
            client_executable(options.flavor).to_owned(),
            "--protocol=socket".to_owned(),
            "--user=root".to_owned(),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
        ],
        BTreeMap::from([(
            "MYSQL_PWD".to_owned(),
            options.administrator.secret().to_owned(),
        )]),
        None,
    )?;
    let command = AttachedCommandOptions::new(
        request,
        deletion_sql(
            options.logical_resource.logical_resource_id(),
            options.credential.username(),
        )
        .into_bytes(),
        format!(
            "prune confirmed {} logical resource",
            implementation(options.flavor)
        ),
        options.timeout,
    )?;

    run_attached_command(executor, options.container, &command).await
}

fn validate(options: &MySqlLogicalPruneOptions<'_>) -> Result<(), EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let expected_kind = match options.flavor {
        MySqlFlavor::MySql => "mysql_database",
        MySqlFlavor::MariaDb => "mariadb_database",
    };
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != expected_kind
        || logical.lifecycle() == ResourceLifecycle::Active
        || logical.orphaned_at_unix_seconds().is_none()
        || !valid_identifier(logical.logical_resource_id(), 64)
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || !valid_identifier(credential.username(), 32)
        || administrator.project_id().is_some()
        || administrator.service_id() != implementation(options.flavor)
        || administrator.username() != "root"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: format!(
                "{} logical prune inputs are not exact, orphaned, and owned",
                implementation(options.flavor)
            ),
        });
    }

    Ok(())
}

fn deletion_sql(schema_name: &str, username: &str) -> String {
    format!(
        "DROP DATABASE IF EXISTS `{schema_name}`;\n\
         DROP USER IF EXISTS '{username}'@'%';\n"
    )
}

fn valid_identifier(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' => index > 0,
            _ => false,
        })
}

const fn implementation(flavor: MySqlFlavor) -> &'static str {
    match flavor {
        MySqlFlavor::MySql => "mysql",
        MySqlFlavor::MariaDb => "mariadb",
    }
}

const fn client_executable(flavor: MySqlFlavor) -> &'static str {
    implementation(flavor)
}
