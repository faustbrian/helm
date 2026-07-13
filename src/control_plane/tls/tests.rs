use super::{
    CertificateTrustStore, DebianCertificateTrustStore, FilesystemCertificateStore, HostCommand,
    HostCommandExecutor, HostCommandOutput, LocalCaIdentity, LocalCertificateReconcileAction,
    MacOsCertificateTrustStore, TrustChange, TrustStoreError, WindowsCertificateTrustStore,
    ensure_ca_trusted, generate_local_certificates, reconcile_local_certificates, remove_ca_trust,
    renew_local_leaf_certificate,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};
use time::macros::datetime;
use x509_parser::extensions::GeneralName;
use x509_parser::parse_x509_certificate;
use x509_parser::pem::parse_x509_pem;

#[test]
fn stackctl_generates_its_own_ca_and_wildcard_gateway_leaf() {
    let now = datetime!(2026-07-13 12:00 UTC);

    let bundle = generate_local_certificates(now).expect("local TLS bundle");
    let (_, leaf_pem) =
        parse_x509_pem(bundle.leaf_certificate_pem().as_bytes()).expect("leaf certificate PEM");
    let (_, leaf) = parse_x509_certificate(&leaf_pem.contents).expect("leaf X.509");
    let (_, ca_pem) =
        parse_x509_pem(bundle.ca_certificate_pem().as_bytes()).expect("CA certificate PEM");
    let (_, ca) = parse_x509_certificate(&ca_pem.contents).expect("CA X.509");

    let wildcard_present = leaf
        .subject_alternative_name()
        .expect("leaf SAN extension")
        .is_some_and(|extension| {
            extension
                .value
                .general_names
                .iter()
                .any(|name| matches!(name, GeneralName::DNSName("*.stackctl.localhost")))
        });

    assert!(wildcard_present);
    assert!(ca.is_ca());
    assert_eq!(leaf.issuer(), ca.subject());
    assert!(bundle.ca_private_key_pem().contains("PRIVATE KEY"));
    assert!(bundle.leaf_private_key_pem().contains("PRIVATE KEY"));
    assert_eq!(bundle.leaf_renew_after(), datetime!(2026-09-26 12:00 UTC));
}

#[test]
fn certificate_bundle_debug_output_redacts_private_material() {
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("local TLS bundle");

    let debug = format!("{bundle:?}");

    assert!(!debug.contains("PRIVATE KEY"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn routine_leaf_renewal_preserves_the_trusted_ca() {
    let original =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("initial TLS bundle");

    let renewed = renew_local_leaf_certificate(&original, datetime!(2026-09-27 12:00 UTC))
        .expect("renew wildcard leaf");

    assert_eq!(renewed.ca_certificate_pem(), original.ca_certificate_pem());
    assert_eq!(renewed.ca_private_key_pem(), original.ca_private_key_pem());
    assert_ne!(
        renewed.leaf_certificate_pem(),
        original.leaf_certificate_pem()
    );
    assert_ne!(
        renewed.leaf_private_key_pem(),
        original.leaf_private_key_pem()
    );
    assert_eq!(renewed.leaf_renew_after(), datetime!(2026-12-11 12:00 UTC));

    let (_, ca_pem) =
        parse_x509_pem(renewed.ca_certificate_pem().as_bytes()).expect("CA certificate PEM");
    let (_, ca) = parse_x509_certificate(&ca_pem.contents).expect("CA X.509");
    let (_, leaf_pem) =
        parse_x509_pem(renewed.leaf_certificate_pem().as_bytes()).expect("leaf certificate PEM");
    let (_, leaf) = parse_x509_certificate(&leaf_pem.contents).expect("leaf X.509");

    assert_eq!(leaf.issuer(), ca.subject());
}

#[test]
fn certificate_reconciliation_generates_material_when_none_exists() {
    let now = datetime!(2026-07-13 12:00 UTC);

    let result = reconcile_local_certificates(None, now).expect("generate certificate material");

    assert_eq!(result.action(), LocalCertificateReconcileAction::Generated);
    assert_eq!(
        result.bundle().leaf_renew_after(),
        datetime!(2026-09-26 12:00 UTC)
    );
}

#[test]
fn certificate_reconciliation_keeps_material_before_renewal_is_due() {
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("certificate bundle");

    let result = reconcile_local_certificates(Some(&bundle), datetime!(2026-09-26 11:59:59 UTC))
        .expect("retain certificate material");

    assert_eq!(result.action(), LocalCertificateReconcileAction::Unchanged);
    assert_eq!(result.bundle(), &bundle);
}

#[test]
fn certificate_reconciliation_renews_only_the_leaf_when_due() {
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("certificate bundle");

    let result = reconcile_local_certificates(Some(&bundle), bundle.leaf_renew_after())
        .expect("renew certificate material");

    assert_eq!(result.action(), LocalCertificateReconcileAction::Renewed);
    assert_eq!(
        result.bundle().ca_certificate_pem(),
        bundle.ca_certificate_pem()
    );
    assert_eq!(
        result.bundle().ca_private_key_pem(),
        bundle.ca_private_key_pem()
    );
    assert_ne!(
        result.bundle().leaf_certificate_pem(),
        bundle.leaf_certificate_pem()
    );
}

#[test]
fn local_ca_identity_uses_the_exact_certificate_der_fingerprint() {
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("local TLS bundle");

    let identity = LocalCaIdentity::from_pem(bundle.ca_certificate_pem()).expect("CA identity");
    let same_identity = LocalCaIdentity::from_pem(&bundle.ca_certificate_pem().replace(
        "-----BEGIN CERTIFICATE-----\n",
        "-----BEGIN CERTIFICATE-----\n\n",
    ))
    .expect("reformatted CA identity");

    assert_eq!(identity, same_identity);
    assert_eq!(identity.sha256_hex().len(), 64);
    assert_eq!(identity.sha1_hex().len(), 40);
    assert!(
        identity
            .sha256_hex()
            .chars()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    assert!(
        identity
            .sha256_hex()
            .chars()
            .all(|byte| !byte.is_ascii_lowercase())
    );
}

#[test]
fn local_ca_identity_rejects_non_ca_certificates() {
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("local TLS bundle");

    let error = LocalCaIdentity::from_pem(bundle.leaf_certificate_pem())
        .expect_err("leaf must not become a trusted CA identity");

    assert_eq!(error.to_string(), "certificate is not a CA");
}

#[test]
fn trust_reconciliation_installs_a_missing_ca_once() {
    let (identity, certificate_path) = trust_fixture();
    let store = RecordingTrustStore::default();

    assert_eq!(
        ensure_ca_trusted(&store, &identity, &certificate_path).expect("install trust"),
        TrustChange::Installed
    );
    assert_eq!(
        ensure_ca_trusted(&store, &identity, &certificate_path).expect("retain trust"),
        TrustChange::Unchanged
    );
    assert_eq!(store.installed.borrow().as_slice(), &[certificate_path]);
}

#[test]
fn trust_removal_targets_only_the_exact_installed_ca() {
    let (identity, certificate_path) = trust_fixture();
    let store = RecordingTrustStore::default();

    assert_eq!(
        remove_ca_trust(&store, &identity).expect("missing trust"),
        TrustChange::Unchanged
    );
    ensure_ca_trusted(&store, &identity, &certificate_path).expect("install trust");
    assert_eq!(
        remove_ca_trust(&store, &identity).expect("remove exact trust"),
        TrustChange::Removed
    );
    assert_eq!(store.removed.borrow().as_slice(), &[identity]);
}

#[test]
fn macos_trust_store_queries_the_exact_sha256_identity() {
    let (identity, _) = trust_fixture();
    let runner = RecordingCommandExecutor::with_outputs([HostCommandOutput::success(format!(
        "SHA-256 hash: {}\n",
        identity.sha256_hex()
    ))]);
    let store = MacOsCertificateTrustStore::new(runner.clone());

    assert!(store.contains(&identity).expect("query Keychain"));
    assert_eq!(
        runner.commands(),
        vec![HostCommand::new(
            "security",
            [
                "find-certificate",
                "-a",
                "-Z",
                "/Library/Keychains/System.keychain",
            ]
        )]
    );
}

#[test]
fn macos_trust_store_installs_and_removes_only_the_exact_ca() {
    let (identity, certificate_path) = trust_fixture();
    let runner = RecordingCommandExecutor::with_outputs([
        HostCommandOutput::success(""),
        HostCommandOutput::success(""),
    ]);
    let store = MacOsCertificateTrustStore::new(runner.clone());

    store
        .install(&identity, &certificate_path)
        .expect("install Keychain CA");
    store.remove(&identity).expect("remove Keychain CA");

    assert_eq!(
        runner.commands(),
        vec![
            HostCommand::new(
                "sudo",
                [
                    "security",
                    "add-trusted-cert",
                    "-d",
                    "-r",
                    "trustRoot",
                    "-k",
                    "/Library/Keychains/System.keychain",
                    certificate_path.to_str().expect("UTF-8 certificate path"),
                ]
            ),
            HostCommand::new(
                "sudo",
                [
                    "security",
                    "delete-certificate",
                    "-Z",
                    identity.sha256_hex(),
                    "/Library/Keychains/System.keychain",
                ]
            ),
        ]
    );
}

#[test]
fn windows_trust_store_queries_the_current_user_root_by_exact_thumbprint() {
    let (identity, _) = trust_fixture();
    let runner = RecordingCommandExecutor::with_outputs([HostCommandOutput::success("")]);
    let store = WindowsCertificateTrustStore::new(runner.clone());

    assert!(store.contains(&identity).expect("query Windows root store"));
    assert_eq!(
        runner.commands(),
        vec![HostCommand::new(
            "certutil",
            ["-user", "-store", "Root", identity.sha1_hex()]
        )]
    );
}

#[test]
fn windows_trust_store_treats_only_not_found_as_missing() {
    let (identity, _) = trust_fixture();
    let runner = RecordingCommandExecutor::with_outputs([HostCommandOutput::failure(
        "CertUtil: -store command FAILED: 0x80092004",
    )]);
    let store = WindowsCertificateTrustStore::new(runner);

    assert!(!store.contains(&identity).expect("missing Windows root"));
}

#[test]
fn windows_trust_store_installs_and_removes_the_current_user_ca() {
    let (identity, certificate_path) = trust_fixture();
    let runner = RecordingCommandExecutor::with_outputs([
        HostCommandOutput::success(""),
        HostCommandOutput::success(""),
    ]);
    let store = WindowsCertificateTrustStore::new(runner.clone());

    store
        .install(&identity, &certificate_path)
        .expect("install Windows CA");
    store.remove(&identity).expect("remove Windows CA");

    assert_eq!(
        runner.commands(),
        vec![
            HostCommand::new(
                "certutil",
                [
                    "-user",
                    "-addstore",
                    "Root",
                    certificate_path.to_str().expect("UTF-8 certificate path"),
                ]
            ),
            HostCommand::new(
                "certutil",
                ["-user", "-delstore", "Root", identity.sha1_hex()]
            ),
        ]
    );
}

#[test]
fn debian_trust_store_verifies_its_fingerprint_named_certificate() {
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("local TLS bundle");
    let identity = LocalCaIdentity::from_pem(bundle.ca_certificate_pem()).expect("CA identity");
    let root = temporary_certificate_root();
    std::fs::create_dir_all(&root).expect("create local CA directory");
    let runner = RecordingCommandExecutor::default();
    let store = DebianCertificateTrustStore::with_local_ca_directory(runner, root.clone());

    assert!(!store.contains(&identity).expect("missing Debian CA"));
    std::fs::write(
        store.managed_certificate_path(&identity),
        bundle.ca_certificate_pem(),
    )
    .expect("write managed Debian CA");
    assert!(store.contains(&identity).expect("matching Debian CA"));

    std::fs::remove_dir_all(root).expect("remove local CA directory");
}

#[test]
fn debian_trust_store_refuses_a_conflicting_managed_file() {
    let first =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("first TLS bundle");
    let second =
        generate_local_certificates(datetime!(2026-07-14 12:00 UTC)).expect("second TLS bundle");
    let identity =
        LocalCaIdentity::from_pem(first.ca_certificate_pem()).expect("first CA identity");
    let root = temporary_certificate_root();
    std::fs::create_dir_all(&root).expect("create local CA directory");
    let store = DebianCertificateTrustStore::with_local_ca_directory(
        RecordingCommandExecutor::default(),
        root.clone(),
    );
    std::fs::write(
        store.managed_certificate_path(&identity),
        second.ca_certificate_pem(),
    )
    .expect("write conflicting Debian CA");

    let error = store
        .contains(&identity)
        .expect_err("conflicting managed CA must fail");

    assert!(
        error
            .to_string()
            .contains("does not match expected fingerprint")
    );
    std::fs::remove_dir_all(root).expect("remove local CA directory");
}

#[test]
fn debian_trust_store_installs_and_removes_through_the_os_mechanism() {
    let (identity, certificate_path) = trust_fixture();
    let runner = RecordingCommandExecutor::with_outputs([
        HostCommandOutput::success(""),
        HostCommandOutput::success(""),
        HostCommandOutput::success(""),
        HostCommandOutput::success(""),
    ]);
    let store = DebianCertificateTrustStore::with_local_ca_directory(
        runner.clone(),
        PathBuf::from("/usr/local/share/ca-certificates"),
    );
    let managed_path = store.managed_certificate_path(&identity);

    store
        .install(&identity, &certificate_path)
        .expect("install Debian CA");
    store.remove(&identity).expect("remove Debian CA");

    assert_eq!(
        runner.commands(),
        vec![
            HostCommand::new(
                "sudo",
                [
                    "install",
                    "-m",
                    "0644",
                    certificate_path.to_str().expect("UTF-8 source path"),
                    managed_path.to_str().expect("UTF-8 managed path"),
                ]
            ),
            HostCommand::new("sudo", ["update-ca-certificates"]),
            HostCommand::new(
                "sudo",
                [
                    "rm",
                    "-f",
                    managed_path.to_str().expect("UTF-8 managed path"),
                ]
            ),
            HostCommand::new("sudo", ["update-ca-certificates", "--fresh"]),
        ]
    );
}

#[cfg(unix)]
#[test]
fn certificate_bundles_persist_as_atomic_user_private_directories() {
    let root = temporary_certificate_root();
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("local TLS bundle");
    let store = FilesystemCertificateStore::new(root.clone());

    let stored = store.persist(&bundle).expect("persist certificate bundle");

    assert_eq!(mode(&root), 0o700);
    assert_eq!(mode(stored.directory()), 0o700);
    assert_eq!(mode(&stored.ca_certificate()), 0o600);
    assert_eq!(mode(&stored.ca_private_key()), 0o600);
    assert_eq!(mode(&stored.leaf_certificate()), 0o600);
    assert_eq!(mode(&stored.leaf_private_key()), 0o600);
    assert_eq!(mode(&stored.renew_after()), 0o600);
    assert_eq!(
        std::fs::read_to_string(stored.leaf_private_key()).expect("stored leaf key"),
        bundle.leaf_private_key_pem()
    );
    assert!(
        std::fs::read_dir(&root)
            .expect("certificate root")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .contains(".tmp"))
    );

    std::fs::remove_dir_all(root).expect("remove certificate test root");
}

#[cfg(unix)]
#[test]
fn certificate_bundles_load_with_their_persisted_renewal_deadline() {
    let root = temporary_certificate_root();
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("local TLS bundle");
    let store = FilesystemCertificateStore::new(root.clone());
    let stored = store.persist(&bundle).expect("persist certificate bundle");

    let reloaded_store = FilesystemCertificateStore::new(root.clone());
    let (loaded, restored) = reloaded_store
        .load_directory(stored.directory())
        .expect("load certificate bundle after restart");

    assert_eq!(loaded, bundle);
    assert_eq!(restored, stored);
    std::fs::remove_dir_all(root).expect("remove certificate test root");
}

#[cfg(unix)]
#[test]
fn certificate_store_recovers_the_latest_verified_bundle_after_restart() {
    let root = temporary_certificate_root();
    let initial =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("initial TLS bundle");
    let renewed = renew_local_leaf_certificate(&initial, datetime!(2026-10-13 12:00 UTC))
        .expect("renewed TLS bundle");
    let store = FilesystemCertificateStore::new(root.clone());
    store.persist(&initial).expect("persist initial bundle");
    let expected = store.persist(&renewed).expect("persist renewed bundle");

    let (current, paths) = FilesystemCertificateStore::new(root.clone())
        .load_current()
        .expect("recover current bundle")
        .expect("stored current bundle");

    assert_eq!(current, renewed);
    assert_eq!(paths, expected);
    std::fs::remove_dir_all(root).expect("remove certificate test root");
}

#[cfg(unix)]
#[test]
fn certificate_store_refuses_unexpected_state_in_its_private_root() {
    let root = temporary_certificate_root();
    std::fs::create_dir_all(&root).expect("certificate root");
    std::fs::write(root.join("current.pem"), "unowned certificate material")
        .expect("unexpected certificate state");

    let error = FilesystemCertificateStore::new(root.clone())
        .load_current()
        .expect_err("unexpected state must block recovery");

    assert!(error.to_string().contains("unexpected entry 'current.pem'"));
    std::fs::remove_dir_all(root).expect("remove certificate test root");
}

#[cfg(unix)]
#[test]
fn certificate_bundle_loading_rejects_a_corrupt_renewal_deadline() {
    let root = temporary_certificate_root();
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("local TLS bundle");
    let store = FilesystemCertificateStore::new(root.clone());
    let stored = store.persist(&bundle).expect("persist certificate bundle");
    std::fs::write(stored.renew_after(), "not-a-timestamp\n").expect("corrupt renewal metadata");

    let error = store
        .load_directory(stored.directory())
        .expect_err("invalid renewal deadline");

    assert!(
        error
            .to_string()
            .contains("renewal deadline is not a valid Unix timestamp")
    );
    std::fs::remove_dir_all(root).expect("remove certificate test root");
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .expect("certificate path metadata")
        .permissions()
        .mode()
        & 0o777
}

fn temporary_certificate_root() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "stackctl-v8-certificates-{}-{unique}",
        std::process::id()
    ))
}

fn trust_fixture() -> (LocalCaIdentity, PathBuf) {
    let bundle =
        generate_local_certificates(datetime!(2026-07-13 12:00 UTC)).expect("local TLS bundle");
    let identity = LocalCaIdentity::from_pem(bundle.ca_certificate_pem()).expect("CA identity");

    (identity, PathBuf::from("/private/stackctl/ca.pem"))
}

#[derive(Default)]
struct RecordingTrustStore {
    trusted: Cell<bool>,
    installed: RefCell<Vec<PathBuf>>,
    removed: RefCell<Vec<LocalCaIdentity>>,
}

impl CertificateTrustStore for RecordingTrustStore {
    fn contains(&self, _identity: &LocalCaIdentity) -> Result<bool, TrustStoreError> {
        Ok(self.trusted.get())
    }

    fn install(
        &self,
        _identity: &LocalCaIdentity,
        certificate_path: &std::path::Path,
    ) -> Result<(), TrustStoreError> {
        self.installed
            .borrow_mut()
            .push(certificate_path.to_owned());
        self.trusted.set(true);

        Ok(())
    }

    fn remove(&self, identity: &LocalCaIdentity) -> Result<(), TrustStoreError> {
        self.removed.borrow_mut().push(identity.clone());
        self.trusted.set(false);

        Ok(())
    }
}

#[derive(Clone, Default)]
struct RecordingCommandExecutor {
    commands: Rc<RefCell<Vec<HostCommand>>>,
    outputs: Rc<RefCell<VecDeque<HostCommandOutput>>>,
}

impl RecordingCommandExecutor {
    fn with_outputs(outputs: impl IntoIterator<Item = HostCommandOutput>) -> Self {
        Self {
            commands: Rc::default(),
            outputs: Rc::new(RefCell::new(outputs.into_iter().collect())),
        }
    }

    fn commands(&self) -> Vec<HostCommand> {
        self.commands.borrow().clone()
    }
}

impl HostCommandExecutor for RecordingCommandExecutor {
    fn execute(&self, command: &HostCommand) -> Result<HostCommandOutput, TrustStoreError> {
        self.commands.borrow_mut().push(command.clone());
        self.outputs
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| TrustStoreError::new("missing recorded command output"))
    }
}
