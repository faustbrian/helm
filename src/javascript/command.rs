use anyhow::Result;

use super::VersionManager;

pub(crate) struct BuildNodeCommandOptions<'a> {
    pub(crate) version_manager: VersionManager,
    pub(crate) node_version: Option<&'a str>,
    pub(crate) command: &'a [String],
}

pub(crate) fn build_node_command(options: BuildNodeCommandOptions<'_>) -> Result<Vec<String>> {
    match options.version_manager {
        VersionManager::System => Ok(options.command.to_vec()),
        VersionManager::Fnm => Ok(build_fnm_command(options)),
        VersionManager::Volta => Ok(build_volta_command(options)),
        VersionManager::Nvm => Ok(build_nvm_command(options)),
    }
}

fn build_fnm_command(options: BuildNodeCommandOptions<'_>) -> Vec<String> {
    let version = options.node_version.expect("node version required for fnm");
    let mut command = vec![
        "fnm".to_owned(),
        "exec".to_owned(),
        "--using".to_owned(),
        version.to_owned(),
        "--".to_owned(),
    ];
    command.extend(options.command.iter().cloned());
    command
}

fn build_volta_command(options: BuildNodeCommandOptions<'_>) -> Vec<String> {
    let version = options
        .node_version
        .expect("node version required for volta");
    let mut command = vec![
        "volta".to_owned(),
        "run".to_owned(),
        "--node".to_owned(),
        version.to_owned(),
    ];
    command.extend(options.command.iter().cloned());
    command
}

fn build_nvm_command(options: BuildNodeCommandOptions<'_>) -> Vec<String> {
    let version = options.node_version.expect("node version required for nvm");
    let joined = options
        .command
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "export NVM_DIR=/usr/local/nvm && . \"$NVM_DIR/nvm.sh\" && nvm install {version} >/dev/null && nvm exec {version} {joined}"
    );

    vec!["bash".to_owned(), "-lc".to_owned(), script]
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::{BuildNodeCommandOptions, build_node_command};
    use crate::javascript::VersionManager;

    #[test]
    fn build_node_command_wraps_fnm_exec() {
        let command = build_node_command(BuildNodeCommandOptions {
            version_manager: VersionManager::Fnm,
            node_version: Some("22"),
            command: &["pnpm".to_owned(), "install".to_owned()],
        })
        .expect("build fnm command");

        assert_eq!(
            command,
            vec![
                "fnm".to_owned(),
                "exec".to_owned(),
                "--using".to_owned(),
                "22".to_owned(),
                "--".to_owned(),
                "pnpm".to_owned(),
                "install".to_owned(),
            ]
        );
    }
}
