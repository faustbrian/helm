//! User-service definitions for login-time daemon watch startup.

mod canonical_watch_dirs;
mod daemon_readiness_error;
mod launchd;
mod restart_service;
mod service_install_snapshot;
mod store_definition;
mod systemd;
mod verify_daemon_service_readiness;

use service_install_snapshot::ServiceInstallSnapshot;

pub(crate) use canonical_watch_dirs::canonical_watch_dirs;
pub(crate) use restart_service::restart_service;
#[cfg(test)]
use restart_service::restart_service_with_readiness;

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(test)]
use std::cell::RefCell;

const LAUNCHD_LABEL: &str = "dev.stackctl.daemon.watch";
const SYSTEMD_UNIT_NAME: &str = "stackctl-daemon-watch.service";

#[cfg(test)]
thread_local! {
    static TEST_SERVICE_HOME: RefCell<Option<String>> = const { RefCell::new(None) };
    static TEST_SERVICE_BINARY: RefCell<Option<String>> = const { RefCell::new(None) };
    static TEST_SERVICE_MANAGER: RefCell<Option<ServiceManager>> = const { RefCell::new(None) };
    static TEST_SERVICE_COMMANDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static TEST_SERVICE_COMMAND_FAILURE: RefCell<Option<String>> = const { RefCell::new(None) };
    static TEST_SERVICE_RUNNING: RefCell<Option<bool>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServiceManager {
    Launchd,
    SystemdUser,
}

#[derive(Debug, Clone)]
pub(crate) struct DaemonServiceInstallOptions {
    pub(crate) watch_dirs: Vec<PathBuf>,
    pub(crate) interval_secs: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct DaemonServiceDefinition {
    pub(crate) manager: ServiceManager,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DaemonServiceStatus {
    pub(crate) manager: ServiceManager,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) installed: bool,
    pub(crate) running: bool,
    pub(crate) responsive: bool,
}

pub(crate) fn install_service(
    options: &DaemonServiceInstallOptions,
) -> Result<DaemonServiceStatus> {
    #[cfg(test)]
    return install_service_with_readiness(options, || Ok(()));

    #[cfg(not(test))]
    install_service_with_readiness(options, verify_daemon_service_readiness::verify)
}

fn install_service_with_readiness(
    options: &DaemonServiceInstallOptions,
    mut verify_readiness: impl FnMut() -> Result<()>,
) -> Result<DaemonServiceStatus> {
    let options = DaemonServiceInstallOptions {
        watch_dirs: canonical_watch_dirs(&options.watch_dirs)?,
        interval_secs: options.interval_secs,
    };
    let definition = service_definition(&options)?;
    if let Some(parent) = definition.path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let was_running =
        definition.path.exists() && service_is_running(definition.manager, &definition.label)?;
    let snapshot = ServiceInstallSnapshot::capture(&definition.path, was_running)?;
    store_definition::store_definition(&definition.path, &definition.contents)?;

    if let Err(error) = activate_service(&definition) {
        if let Err(rollback) = rollback_service_install(&definition, snapshot) {
            bail!("service activation failed: {error}; rollback failed: {rollback}");
        }

        return Err(error);
    }

    if let Err(error) = verify_readiness() {
        let restore_running_service = snapshot.was_running();
        if let Err(rollback) = rollback_service_install(&definition, snapshot) {
            bail!("daemon readiness failed: {error}; rollback failed: {rollback}");
        }
        if restore_running_service && let Err(rollback) = verify_readiness() {
            bail!(
                "daemon readiness failed: {error}; restored-service readiness failed: {rollback}"
            );
        }

        return Err(error);
    }

    Ok(DaemonServiceStatus {
        manager: definition.manager,
        label: definition.label,
        path: definition.path,
        installed: true,
        running: true,
        responsive: true,
    })
}

fn activate_service(definition: &DaemonServiceDefinition) -> Result<()> {
    match definition.manager {
        ServiceManager::Launchd => install_launchd(definition)?,
        ServiceManager::SystemdUser => install_systemd(definition)?,
    }
    set_test_service_running_state(true);
    if service_is_running(definition.manager, &definition.label)? {
        return Ok(());
    }

    bail!(
        "{} accepted the Stackctl service definition but did not keep '{}' running",
        manager_name(definition.manager),
        definition.label
    )
}

fn rollback_service_install(
    definition: &DaemonServiceDefinition,
    snapshot: ServiceInstallSnapshot,
) -> Result<()> {
    let was_running = snapshot.was_running();
    snapshot.restore(&definition.path)?;
    if was_running {
        activate_service(definition)
    } else {
        match definition.manager {
            ServiceManager::Launchd => uninstall_launchd(&definition.path),
            ServiceManager::SystemdUser => uninstall_systemd(&definition.path),
        }
    }
}

pub(crate) fn uninstall_service() -> Result<DaemonServiceStatus> {
    #[cfg(test)]
    return uninstall_service_with_verification(|_, _| Ok(()));

    #[cfg(not(test))]
    uninstall_service_with_verification(verify_manager_removal)
}

fn uninstall_service_with_verification(
    verify_removal: impl FnOnce(ServiceManager, &str) -> Result<()>,
) -> Result<DaemonServiceStatus> {
    let manager = service_manager()?;
    let definition = service_definition(&DaemonServiceInstallOptions {
        watch_dirs: Vec::new(),
        interval_secs: 30,
    })?;
    let installed = definition.path.exists();

    match manager {
        ServiceManager::Launchd => uninstall_launchd(&definition.path)?,
        ServiceManager::SystemdUser => uninstall_systemd(&definition.path)?,
    }
    verify_removal(manager, &definition.label)?;
    set_test_service_running_state(false);

    if definition.path.exists() {
        fs::remove_file(&definition.path)
            .with_context(|| format!("failed to remove {}", definition.path.display()))?;
    }

    Ok(DaemonServiceStatus {
        manager,
        label: definition.label,
        path: definition.path,
        installed,
        running: false,
        responsive: false,
    })
}

#[cfg_attr(test, allow(dead_code))]
fn verify_manager_removal(manager: ServiceManager, label: &str) -> Result<()> {
    if service_is_running(manager, label)? {
        bail!(
            "{} still reports '{}' as running after removal",
            manager_name(manager),
            label
        );
    }
    if manager == ServiceManager::SystemdUser && service_is_enabled(label)? {
        bail!("systemd --user still reports '{label}' as enabled after removal");
    }

    Ok(())
}

pub(crate) fn service_status() -> Result<DaemonServiceStatus> {
    #[cfg(test)]
    return service_status_with_readiness(|| true);

    #[cfg(not(test))]
    service_status_with_readiness(|| verify_daemon_service_readiness::probe().is_ok())
}

fn service_status_with_readiness(readiness: impl FnOnce() -> bool) -> Result<DaemonServiceStatus> {
    let definition = service_definition(&DaemonServiceInstallOptions {
        watch_dirs: Vec::new(),
        interval_secs: 30,
    })?;
    let installed = definition.path.exists();
    let running = installed && service_is_running(definition.manager, &definition.label)?;
    let responsive = running && readiness();
    Ok(DaemonServiceStatus {
        manager: definition.manager,
        label: definition.label,
        installed,
        running,
        responsive,
        path: definition.path,
    })
}

fn service_is_running(manager: ServiceManager, label: &str) -> Result<bool> {
    match manager {
        ServiceManager::Launchd => run_status(
            "launchctl",
            &[
                "print".to_owned(),
                format!("{}/{}", launchd_domain()?, label),
            ],
        ),
        ServiceManager::SystemdUser => run_status(
            "systemctl",
            &[
                "--user".to_owned(),
                "is-active".to_owned(),
                "--quiet".to_owned(),
                label.to_owned(),
            ],
        ),
    }
}

#[cfg_attr(test, allow(dead_code))]
fn service_is_enabled(label: &str) -> Result<bool> {
    run_status(
        "systemctl",
        &[
            "--user".to_owned(),
            "is-enabled".to_owned(),
            "--quiet".to_owned(),
            label.to_owned(),
        ],
    )
}

fn manager_name(manager: ServiceManager) -> &'static str {
    match manager {
        ServiceManager::Launchd => "launchd",
        ServiceManager::SystemdUser => "systemd --user",
    }
}

pub(crate) fn print_service(
    options: &DaemonServiceInstallOptions,
) -> Result<DaemonServiceDefinition> {
    service_definition(options)
}

fn service_definition(options: &DaemonServiceInstallOptions) -> Result<DaemonServiceDefinition> {
    let manager = service_manager()?;
    match manager {
        ServiceManager::Launchd => launchd::definition(
            &service_context(options)?,
            LAUNCHD_LABEL,
            &launchd_plist_path()?,
        ),
        ServiceManager::SystemdUser => systemd::definition(
            &service_context(options)?,
            SYSTEMD_UNIT_NAME,
            &systemd_unit_path()?,
        ),
    }
}

fn service_context(options: &DaemonServiceInstallOptions) -> Result<ServiceContext> {
    let binary = daemon_service_binary()?;
    let args = watch_command_args(options);
    Ok(ServiceContext { binary, args })
}

fn watch_command_args(options: &DaemonServiceInstallOptions) -> Vec<String> {
    let mut args = vec![
        "daemon".to_owned(),
        "watch".to_owned(),
        "--interval".to_owned(),
        options.interval_secs.max(1).to_string(),
    ];
    for dir in &options.watch_dirs {
        args.push("--dir".to_owned());
        args.push(dir.to_string_lossy().into_owned());
    }
    args
}

fn install_launchd(definition: &DaemonServiceDefinition) -> Result<()> {
    let domain = launchd_domain()?;
    run_command(
        "launchctl",
        &[
            "bootout".to_owned(),
            domain.clone(),
            definition.path.to_string_lossy().into_owned(),
        ],
        true,
    )?;
    run_command(
        "launchctl",
        &[
            "bootstrap".to_owned(),
            domain,
            definition.path.to_string_lossy().into_owned(),
        ],
        false,
    )
}

fn uninstall_launchd(path: &Path) -> Result<()> {
    let domain = launchd_domain()?;
    run_command(
        "launchctl",
        &[
            "bootout".to_owned(),
            domain,
            path.to_string_lossy().into_owned(),
        ],
        true,
    )
}

fn install_systemd(definition: &DaemonServiceDefinition) -> Result<()> {
    run_command(
        "systemctl",
        &["--user".to_owned(), "daemon-reload".to_owned()],
        false,
    )?;
    run_command(
        "systemctl",
        &[
            "--user".to_owned(),
            "enable".to_owned(),
            "--now".to_owned(),
            definition.label.clone(),
        ],
        false,
    )
}

fn uninstall_systemd(path: &Path) -> Result<()> {
    let label = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(SYSTEMD_UNIT_NAME)
        .to_owned();
    run_command(
        "systemctl",
        &[
            "--user".to_owned(),
            "disable".to_owned(),
            "--now".to_owned(),
            label,
        ],
        true,
    )?;
    run_command(
        "systemctl",
        &["--user".to_owned(), "daemon-reload".to_owned()],
        true,
    )
}

fn run_command(program: &str, args: &[String], allow_failure: bool) -> Result<()> {
    #[cfg(test)]
    if test_mode_enabled() {
        let rendered = record_test_command(program, args);
        let should_fail = TEST_SERVICE_COMMAND_FAILURE.with(|failure| {
            if failure.borrow().as_deref() != Some(rendered.as_str()) {
                return false;
            }
            failure.borrow_mut().take();
            true
        });
        if should_fail {
            bail!("test service command failed: {rendered}");
        }
        return Ok(());
    }

    let mut command = Command::new(program);
    command.args(args);
    if allow_failure {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let status = command
        .status()
        .with_context(|| format!("failed to run {}", program))?;
    if status.success() || allow_failure {
        return Ok(());
    }

    bail!("{} exited with {}", program, status);
}

fn run_status(program: &str, args: &[String]) -> Result<bool> {
    #[cfg(test)]
    if test_mode_enabled() {
        record_test_command(program, args);
        return Ok(TEST_SERVICE_RUNNING.with(|running| running.borrow().unwrap_or(false)));
    }

    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to run {program}"))
        .map(|status| status.success())
}

#[cfg(test)]
fn record_test_command(program: &str, args: &[String]) -> String {
    let rendered = std::iter::once(program.to_owned())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    TEST_SERVICE_COMMANDS.with(|commands| {
        commands.borrow_mut().push(rendered.clone());
    });
    rendered
}

#[cfg(not(test))]
const fn set_test_service_running_state(_running: bool) {}

#[cfg(test)]
fn set_test_service_running_state(running: bool) {
    TEST_SERVICE_RUNNING.with(|state| *state.borrow_mut() = Some(running));
}

fn launchd_domain() -> Result<String> {
    Ok(format_launchd_domain(rustix::process::geteuid().as_raw()))
}

fn format_launchd_domain(uid: u32) -> String {
    format!("gui/{uid}")
}

fn service_manager() -> Result<ServiceManager> {
    #[cfg(test)]
    if let Some(manager) = TEST_SERVICE_MANAGER.with(|value| *value.borrow()) {
        return Ok(manager);
    }

    if cfg!(target_os = "macos") {
        return Ok(ServiceManager::Launchd);
    }
    if cfg!(target_os = "linux") {
        return Ok(ServiceManager::SystemdUser);
    }

    bail!("daemon services are only supported on macOS and Linux");
}

fn daemon_service_binary() -> Result<String> {
    #[cfg(test)]
    if let Some(binary) = TEST_SERVICE_BINARY.with(|value| value.borrow().clone()) {
        return Ok(binary);
    }

    std::env::current_exe()
        .context("failed to resolve current stackctl executable")
        .map(|path| path.to_string_lossy().into_owned())
}

fn home_dir() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(home) = TEST_SERVICE_HOME.with(|value| value.borrow().clone()) {
        return Ok(PathBuf::from(home));
    }

    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home))
}

fn launchd_plist_path() -> Result<PathBuf> {
    Ok(home_dir()?
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

fn systemd_unit_path() -> Result<PathBuf> {
    Ok(home_dir()?
        .join(".config")
        .join("systemd")
        .join("user")
        .join(SYSTEMD_UNIT_NAME))
}

#[cfg(test)]
fn test_mode_enabled() -> bool {
    TEST_SERVICE_HOME.with(|value| value.borrow().is_some())
        || TEST_SERVICE_BINARY.with(|value| value.borrow().is_some())
        || TEST_SERVICE_MANAGER.with(|value| value.borrow().is_some())
}

#[cfg(test)]
pub(crate) fn set_test_service_home(home: &str) {
    TEST_SERVICE_HOME.with(|value| *value.borrow_mut() = Some(home.to_owned()));
}

#[cfg(test)]
pub(crate) fn clear_test_service_home() {
    TEST_SERVICE_HOME.with(|value| *value.borrow_mut() = None);
    TEST_SERVICE_RUNNING.with(|value| *value.borrow_mut() = None);
    TEST_SERVICE_COMMAND_FAILURE.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
pub(crate) fn set_test_service_binary(path: &str) {
    TEST_SERVICE_BINARY.with(|value| *value.borrow_mut() = Some(path.to_owned()));
}

#[cfg(test)]
pub(crate) fn clear_test_service_binary() {
    TEST_SERVICE_BINARY.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
pub(crate) fn set_test_service_manager(manager: ServiceManager) {
    TEST_SERVICE_MANAGER.with(|value| *value.borrow_mut() = Some(manager));
}

#[cfg(test)]
pub(crate) fn clear_test_service_manager() {
    TEST_SERVICE_MANAGER.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
pub(crate) fn clear_test_service_commands() {
    TEST_SERVICE_COMMANDS.with(|value| value.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn take_test_service_commands() -> Vec<String> {
    TEST_SERVICE_COMMANDS.with(|value| std::mem::take(&mut *value.borrow_mut()))
}

#[cfg(test)]
pub(crate) fn set_test_service_running(running: bool) {
    set_test_service_running_state(running);
}

#[cfg(test)]
pub(crate) fn set_test_service_command_failure(command: &str) {
    TEST_SERVICE_COMMAND_FAILURE.with(|failure| {
        *failure.borrow_mut() = Some(command.to_owned());
    });
}

struct ServiceContext {
    binary: String,
    args: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        DaemonServiceInstallOptions, ServiceManager, clear_test_service_binary,
        clear_test_service_commands, clear_test_service_home, clear_test_service_manager,
        format_launchd_domain, install_service, install_service_with_readiness, print_service,
        restart_service_with_readiness, service_status, service_status_with_readiness,
        set_test_service_binary, set_test_service_command_failure, set_test_service_home,
        set_test_service_manager, set_test_service_running, take_test_service_commands,
        uninstall_service, uninstall_service_with_verification,
    };
    use std::cell::Cell;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "stackctl-daemon-service-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn service_options() -> DaemonServiceInstallOptions {
        DaemonServiceInstallOptions {
            watch_dirs: vec![std::env::temp_dir()],
            interval_secs: 45,
        }
    }

    #[test]
    fn launchd_domain_uses_the_numeric_uid_directly() {
        assert_eq!(format_launchd_domain(501), "gui/501");
    }

    #[cfg(unix)]
    #[test]
    fn install_service_replaces_a_definition_symlink_without_following_it() {
        use std::os::unix::fs::symlink;

        let home = temp_home("definition-symlink");
        let victim = home.join("victim.txt");
        fs::write(&victim, "do not replace\n").expect("write victim");
        let definition = home
            .join("Library")
            .join("LaunchAgents")
            .join("dev.stackctl.daemon.watch.plist");
        fs::create_dir_all(definition.parent().expect("definition parent"))
            .expect("create definition parent");
        symlink(&victim, &definition).expect("create definition symlink");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::Launchd);
        clear_test_service_commands();

        install_service(&service_options()).expect("install launchd");

        assert_eq!(
            fs::read_to_string(&victim).expect("read victim"),
            "do not replace\n"
        );
        assert!(
            !fs::symlink_metadata(&definition)
                .expect("definition metadata")
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::read_to_string(&definition)
                .expect("read definition")
                .contains("/tmp/stackctl")
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn install_service_writes_launchd_plist_and_records_commands() {
        let home = temp_home("launchd");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::Launchd);
        clear_test_service_commands();

        let status = install_service(&service_options()).expect("install launchd");
        let plist = fs::read_to_string(&status.path).expect("read plist");

        assert!(status.installed);
        assert!(status.running);
        assert!(plist.contains("/tmp/stackctl"));
        assert!(plist.contains("<string>--dir</string>"));
        assert!(plist.contains("<string>/dev/null</string>"));
        assert!(plist.contains("<key>ThrottleInterval</key>\n  <integer>30</integer>"));
        assert!(!plist.contains("watch-service.stdout.log"));
        assert!(!plist.contains("watch-service.stderr.log"));
        let watched_root = fs::canonicalize(std::env::temp_dir()).expect("watched root");
        assert!(plist.contains(&format!("<string>{}</string>", watched_root.display())));

        let commands = take_test_service_commands();
        assert_eq!(commands.len(), 3);
        assert!(commands[0].contains("launchctl bootout"));
        assert!(commands[1].contains("launchctl bootstrap"));
        assert!(commands[2].contains("launchctl print"));

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn install_service_writes_systemd_unit_and_records_commands() {
        let home = temp_home("systemd");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();

        let status = install_service(&service_options()).expect("install systemd");
        let unit = fs::read_to_string(&status.path).expect("read unit");

        assert!(status.installed);
        assert!(status.running);
        assert!(unit.contains("ExecStart="));
        assert!(unit.contains("/tmp/stackctl"));
        assert!(unit.contains("--interval"));
        assert!(!unit.contains("--exclude-dir"));
        assert!(!unit.contains("--max-projects"));
        assert!(unit.contains("StandardOutput=journal"));
        assert!(unit.contains("StandardError=journal"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("RestartSec=30"));
        assert!(!unit.contains("Restart=always"));
        assert!(!unit.contains("StandardOutput=append:"));
        assert!(!unit.contains("StandardError=append:"));

        let commands = take_test_service_commands();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0], "systemctl --user daemon-reload");
        assert_eq!(
            commands[1],
            "systemctl --user enable --now stackctl-daemon-watch.service"
        );
        assert_eq!(
            commands[2],
            "systemctl --user is-active --quiet stackctl-daemon-watch.service"
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn restart_service_uses_systemd_and_requires_daemon_readiness() {
        let home = temp_home("systemd-restart");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, definition.contents).expect("write definition");
        set_test_service_running(true);
        clear_test_service_commands();

        let status = restart_service_with_readiness(|| Ok(())).expect("restart service");

        assert!(status.installed);
        assert!(status.running);
        assert!(status.responsive);
        assert_eq!(
            take_test_service_commands(),
            [
                "systemctl --user restart stackctl-daemon-watch.service",
                "systemctl --user is-active --quiet stackctl-daemon-watch.service",
            ]
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn restart_service_uses_launchd_kickstart_for_the_user_domain() {
        let home = temp_home("launchd-restart");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::Launchd);
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, definition.contents).expect("write definition");
        set_test_service_running(true);
        clear_test_service_commands();

        restart_service_with_readiness(|| Ok(())).expect("restart service");

        assert_eq!(
            take_test_service_commands(),
            [
                format!(
                    "launchctl kickstart -k gui/{}/dev.stackctl.daemon.watch",
                    rustix::process::geteuid().as_raw()
                ),
                format!(
                    "launchctl print gui/{}/dev.stackctl.daemon.watch",
                    rustix::process::geteuid().as_raw()
                ),
            ]
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn restart_service_reports_failed_operational_readiness() {
        let home = temp_home("restart-readiness");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, definition.contents).expect("write definition");
        set_test_service_running(true);
        clear_test_service_commands();

        let error = restart_service_with_readiness(|| anyhow::bail!("project is unhealthy"))
            .expect_err("failed readiness");

        assert!(
            error
                .to_string()
                .contains("restarted daemon did not become operationally ready")
        );
        assert!(
            error
                .chain()
                .any(|cause| cause.to_string() == "project is unhealthy")
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn restart_service_rejects_a_missing_definition_without_manager_mutation() {
        let home = temp_home("missing-restart");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();

        let error = restart_service_with_readiness(|| Ok(())).expect_err("missing service");

        assert!(error.to_string().contains("is not installed"));
        assert!(take_test_service_commands().is_empty());

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[cfg(unix)]
    #[test]
    fn restart_service_refuses_a_linked_definition() {
        use std::os::unix::fs::symlink;

        let home = temp_home("linked-restart");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        let victim = home.join("victim.service");
        fs::write(&victim, "foreign service\n").expect("write victim");
        symlink(&victim, &definition.path).expect("link definition");
        clear_test_service_commands();

        let error = restart_service_with_readiness(|| Ok(())).expect_err("linked definition");

        assert!(error.to_string().contains("is not a real file"));
        assert!(take_test_service_commands().is_empty());
        assert_eq!(
            fs::read_to_string(&victim).expect("read victim"),
            "foreign service\n"
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn install_service_rejects_missing_watch_roots_before_host_mutation() {
        let home = temp_home("missing-watch-root");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        let options = DaemonServiceInstallOptions {
            watch_dirs: vec![home.join("missing")],
            interval_secs: 45,
        };
        let definition = print_service(&options).expect("service definition");

        let error = install_service(&options).expect_err("missing watch root");

        assert!(error.to_string().contains("watched root"));
        assert!(!definition.path.exists());
        assert!(take_test_service_commands().is_empty());

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn uninstall_service_removes_installed_definition() {
        let home = temp_home("uninstall");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();

        let installed = install_service(&service_options()).expect("install");
        assert!(installed.path.exists());
        let removed = uninstall_service().expect("uninstall");

        assert!(removed.installed);
        assert!(!removed.running);
        assert!(!removed.path.exists());

        let commands = take_test_service_commands();
        assert!(commands.iter().any(|command| {
            command == "systemctl --user disable --now stackctl-daemon-watch.service"
        }));

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn failed_manager_removal_verification_preserves_the_service_definition() {
        let home = temp_home("uninstall-verification");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, &definition.contents).expect("write definition");

        let error = uninstall_service_with_verification(|_, _| {
            anyhow::bail!("systemd unit remains enabled")
        })
        .expect_err("unverified manager cleanup");

        assert!(error.to_string().contains("remains enabled"));
        assert!(definition.path.exists());

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn print_and_status_report_service_definition() {
        let home = temp_home("status");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::Launchd);

        let printed = print_service(&service_options()).expect("print");
        assert_eq!(printed.label, "dev.stackctl.daemon.watch");
        assert!(printed.contents.contains("/tmp/stackctl"));

        let status_before = service_status().expect("status before install");
        assert!(!status_before.installed);
        assert!(!status_before.running);

        install_service(&service_options()).expect("install");
        let status_after = service_status().expect("status after install");
        assert!(status_after.installed);
        assert!(status_after.running);

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn status_distinguishes_a_stale_definition_from_a_running_service() {
        let home = temp_home("stale-status");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, definition.contents).expect("write stale definition");
        set_test_service_running(false);

        let status = service_status().expect("stale service status");

        assert!(status.installed);
        assert!(!status.running);
        assert_eq!(
            take_test_service_commands(),
            ["systemctl --user is-active --quiet stackctl-daemon-watch.service"]
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn status_distinguishes_a_running_but_unresponsive_daemon() {
        let home = temp_home("unresponsive-status");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, definition.contents).expect("write definition");
        set_test_service_running(true);

        let status = service_status_with_readiness(|| false).expect("service status");

        assert!(status.installed);
        assert!(status.running);
        assert!(!status.responsive);

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn failed_fresh_install_removes_its_definition_and_partial_manager_state() {
        let home = temp_home("fresh-install-rollback");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        set_test_service_command_failure(
            "systemctl --user enable --now stackctl-daemon-watch.service",
        );
        let definition = print_service(&service_options()).expect("service definition");

        let error = install_service(&service_options()).expect_err("failed activation");

        assert!(error.to_string().contains("enable --now"));
        assert!(!definition.path.exists());
        assert!(take_test_service_commands().iter().any(|command| {
            command == "systemctl --user disable --now stackctl-daemon-watch.service"
        }));

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn failed_update_restores_and_restarts_the_previous_service_definition() {
        let home = temp_home("update-install-rollback");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, "previous service definition\n")
            .expect("write previous definition");
        set_test_service_running(true);
        set_test_service_command_failure(
            "systemctl --user enable --now stackctl-daemon-watch.service",
        );

        let error = install_service(&service_options()).expect_err("failed update");

        assert!(error.to_string().contains("enable --now"));
        assert_eq!(
            fs::read_to_string(&definition.path).expect("restored definition"),
            "previous service definition\n"
        );
        let commands = take_test_service_commands();
        assert_eq!(
            commands
                .iter()
                .filter(|command| {
                    command.as_str()
                        == "systemctl --user enable --now stackctl-daemon-watch.service"
                })
                .count(),
            2
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn failed_daemon_readiness_restores_the_previous_service_definition() {
        let home = temp_home("update-readiness-rollback");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, "previous service definition\n")
            .expect("write previous definition");
        set_test_service_running(true);
        let readiness_attempts = Cell::new(0_u8);

        let error = install_service_with_readiness(&service_options(), || {
            readiness_attempts.set(readiness_attempts.get() + 1);
            if readiness_attempts.get() == 1 {
                anyhow::bail!("daemon did not answer ping")
            }
            Ok(())
        })
        .expect_err("failed daemon readiness");

        assert!(error.to_string().contains("daemon did not answer ping"));
        assert_eq!(
            fs::read_to_string(&definition.path).expect("restored definition"),
            "previous service definition\n"
        );
        let commands = take_test_service_commands();
        assert_eq!(
            commands
                .iter()
                .filter(|command| {
                    command.as_str()
                        == "systemctl --user enable --now stackctl-daemon-watch.service"
                })
                .count(),
            2
        );
        assert_eq!(readiness_attempts.get(), 2);

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn failed_daemon_readiness_removes_a_fresh_service_installation() {
        let home = temp_home("fresh-readiness-rollback");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        let definition = print_service(&service_options()).expect("service definition");

        let error = install_service_with_readiness(&service_options(), || {
            anyhow::bail!("daemon did not answer ping")
        })
        .expect_err("failed daemon readiness");

        assert!(error.to_string().contains("daemon did not answer ping"));
        assert!(!definition.path.exists());
        assert!(take_test_service_commands().iter().any(|command| {
            command == "systemctl --user disable --now stackctl-daemon-watch.service"
        }));

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn failed_restored_service_readiness_reports_both_failures() {
        let home = temp_home("failed-restored-readiness");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/stackctl");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();
        let definition = print_service(&service_options()).expect("service definition");
        fs::create_dir_all(definition.path.parent().expect("definition parent"))
            .expect("create definition parent");
        fs::write(&definition.path, "previous service definition\n")
            .expect("write previous definition");
        set_test_service_running(true);
        let readiness_attempts = Cell::new(0_u8);

        let error = install_service_with_readiness(&service_options(), || {
            readiness_attempts.set(readiness_attempts.get() + 1);
            anyhow::bail!("ping failure {}", readiness_attempts.get())
        })
        .expect_err("failed daemon readiness and rollback readiness");

        assert!(
            error
                .to_string()
                .contains("daemon readiness failed: ping failure 1")
        );
        assert!(
            error
                .to_string()
                .contains("restored-service readiness failed: ping failure 2")
        );
        assert_eq!(
            fs::read_to_string(&definition.path).expect("restored definition"),
            "previous service definition\n"
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }
}
