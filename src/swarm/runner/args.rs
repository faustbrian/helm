use crate::cli::args::PortStrategyArg;

pub(super) fn swarm_child_args(
    target: &super::super::targets::ResolvedSwarmTarget,
    command: &[String],
    force_random_ports: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    write_env: bool,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    repro: bool,
    runtime_env: Option<&str>,
) -> Vec<String> {
    let mut args = Vec::new();
    if quiet {
        args.push("--quiet".to_owned());
    }
    if no_color {
        args.push("--no-color".to_owned());
    }
    if dry_run {
        args.push("--dry-run".to_owned());
    }
    if let Some(env_name) = runtime_env {
        args.push("--env".to_owned());
        args.push(env_name.to_owned());
    }
    args.push("--project-root".to_owned());
    args.push(target.root.display().to_string());
    let mut child_command = command.to_vec();
    if should_enable_random_ports(&child_command, repro) {
        child_command.push("--random-ports".to_owned());
    }
    if should_set_port_strategy(&child_command, port_strategy, repro) {
        child_command.push("--port-strategy".to_owned());
        child_command.push("stable".to_owned());
    }
    if should_set_port_seed(&child_command, port_seed, repro) {
        child_command.push("--port-seed".to_owned());
        child_command.push(port_seed.unwrap_or_default().to_owned());
    }
    if should_force_random_ports(&child_command, force_random_ports, repro) {
        child_command.push("--force-random-ports".to_owned());
    }
    if should_write_env(&child_command, write_env) {
        child_command.push("--write-env".to_owned());
    }
    args.extend(child_command);
    args
}

fn should_enable_random_ports(command: &[String], _repro: bool) -> bool {
    command
        .first()
        .is_some_and(|subcommand| matches!(subcommand.as_str(), "up" | "recreate"))
        && !command.iter().any(|arg| arg == "--random-ports")
}

fn should_force_random_ports(command: &[String], force_random_ports: bool, _repro: bool) -> bool {
    force_random_ports
        && command.first().is_some_and(|subcommand| subcommand == "up")
        && !command.iter().any(|arg| arg == "--force-random-ports")
}

fn should_set_port_strategy(
    command: &[String],
    port_strategy: PortStrategyArg,
    _repro: bool,
) -> bool {
    port_strategy == PortStrategyArg::Stable
        && command.first().is_some_and(|subcommand| subcommand == "up")
        && !command.iter().any(|arg| arg == "--port-strategy")
}

fn should_set_port_seed(command: &[String], port_seed: Option<&str>, _repro: bool) -> bool {
    port_seed.is_some()
        && command.first().is_some_and(|subcommand| subcommand == "up")
        && !command.iter().any(|arg| arg == "--port-seed")
}

fn should_write_env(command: &[String], write_env: bool) -> bool {
    write_env
        && command.first().is_some_and(|subcommand| subcommand == "up")
        && !command.iter().any(|arg| arg == "--write-env")
}
