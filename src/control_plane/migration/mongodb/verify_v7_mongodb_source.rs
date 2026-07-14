use super::{V7MongoDbCredential, mongodb_connection_uri::mongodb_connection_uri};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandRequest, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use std::collections::BTreeMap;
use std::time::Duration;

const VERIFY_SCRIPT: &str = "const result = db.runCommand({ connectionStatus: 1 }); print(db.getName()); print(result.authInfo.authenticatedUsers[0].user); print(db.runCommand({ ping: 1 }).ok);";
const VERIFY_COMMAND: &str =
    "set -eu\nexec mongosh \"$STACKCTL_MONGODB_URI\" --quiet --eval \"$STACKCTL_MONGODB_SCRIPT\"";

/// Proves the retained v7 database remains reachable by its accepted user.
pub(super) async fn verify_v7_mongodb_source(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7MongoDbCredential,
    database_name: &str,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = verification_request(
        credential.username(),
        credential.password(),
        database_name,
        credential.authentication_database(),
    )?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify retained v7 MongoDB source",
        timeout,
    )
    .map_err(|error| operation_error("v7 MongoDB verification is invalid", error))?;
    let output = run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("verify retained v7 MongoDB source", error))?;
    let expected = format!("{database_name}\n{}\n1\n", credential.username());
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "retained v7 MongoDB source returned unexpected identity evidence",
        ));
    }
    Ok(())
}

pub(super) fn verification_request(
    username: &str,
    password: &str,
    database_name: &str,
    authentication_database: &str,
) -> Result<CommandRequest, MigrationOperationError> {
    CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), VERIFY_COMMAND.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_MONGODB_URI".to_owned(),
                mongodb_connection_uri(username, password, database_name, authentication_database),
            ),
            (
                "STACKCTL_MONGODB_SCRIPT".to_owned(),
                VERIFY_SCRIPT.to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("MongoDB verification request is invalid", error))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
