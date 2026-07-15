use super::InstallationInitializationError;
use crate::control_plane::state::{EngineProvider, InstallationRecord, StateStore};
use std::path::PathBuf;

const INSTALLATION_ID_BYTES: usize = 16;

/// Initializes or loads the immutable per-user Docker installation contract.
pub(crate) fn initialize_default_installation<Store>(
    store: &mut Store,
) -> Result<InstallationRecord, InstallationInitializationError>
where
    Store: StateStore,
{
    if let Some(installation) = store.installation()? {
        return Ok(installation);
    }

    let mut identity = [0_u8; INSTALLATION_ID_BYTES];
    getrandom::fill(&mut identity).map_err(InstallationInitializationError::entropy)?;
    let installation = InstallationRecord::new(
        format!("s8-{}", hex::encode(identity)),
        EngineProvider::Docker,
        default_docker_socket()?.to_string_lossy().into_owned(),
    );
    store.initialize_installation(&installation)?;

    Ok(installation)
}

fn default_docker_socket() -> Result<PathBuf, InstallationInitializationError> {
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| InstallationInitializationError::environment("HOME is not set"))?;
        let home = PathBuf::from(home);
        if !home.is_absolute() {
            return Err(InstallationInitializationError::environment(
                "HOME must be an absolute path",
            ));
        }

        return Ok(home.join(".docker/run/docker.sock"));
    }

    Ok(PathBuf::from("/var/run/docker.sock"))
}
