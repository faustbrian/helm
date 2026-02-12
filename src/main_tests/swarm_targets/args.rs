use super::*;

#[test]
fn resolve_swarm_root_handles_relative_and_absolute_paths() {
    let workspace = PathBuf::from("/tmp/workspace");
    let relative = resolve_swarm_root(&workspace, &PathBuf::from("api"));
    assert_eq!(relative, PathBuf::from("/tmp/workspace/api"));

    let absolute_input = PathBuf::from("/tmp/elsewhere");
    let absolute = resolve_swarm_root(&workspace, &absolute_input);
    assert_eq!(absolute, absolute_input);
}

#[test]
fn swarm_child_args_forwards_global_flags_and_project_root() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned(), "--profile".to_owned(), "infra".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        false,
        PortStrategyArg::Random,
        None,
        false,
        true,
        true,
        true,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--quiet",
            "--no-color",
            "--dry-run",
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--profile",
            "infra",
            "--random-ports",
        ]
    );
}

#[test]
fn swarm_child_args_forwards_runtime_env_flag() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        false,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        false,
        Some("test"),
    );

    assert_eq!(
        args,
        vec![
            "--env",
            "test",
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--random-ports",
        ]
    );
}

#[test]
fn swarm_child_args_does_not_duplicate_random_ports_for_up() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned(), "--random-ports".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        false,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec!["--project-root", "/tmp/ws/api", "up", "--random-ports",]
    );
}

#[test]
fn swarm_child_args_does_not_add_random_ports_for_non_up_commands() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["down".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        true,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(args, vec!["--project-root", "/tmp/ws/api", "down"]);
}

#[test]
fn swarm_child_args_adds_force_random_ports_for_up_when_requested() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        true,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--random-ports",
            "--force-random-ports",
        ]
    );
}

#[test]
fn swarm_child_args_adds_write_env_for_up_when_requested() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        false,
        PortStrategyArg::Random,
        None,
        true,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--random-ports",
            "--write-env",
        ]
    );
}

#[test]
fn swarm_child_args_keeps_random_ports_in_repro_mode() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        false,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        true,
        None,
    );

    assert_eq!(
        args,
        vec!["--project-root", "/tmp/ws/api", "up", "--random-ports",]
    );
}

#[test]
fn swarm_child_args_adds_stable_port_strategy_flags() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        false,
        PortStrategyArg::Stable,
        Some("workspace-seed"),
        false,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--random-ports",
            "--port-strategy",
            "stable",
            "--port-seed",
            "workspace-seed",
        ]
    );
}

#[test]
fn swarm_child_args_adds_random_ports_for_recreate() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["recreate".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        false,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "recreate",
            "--random-ports",
        ]
    );
}
