use super::{
    FilesystemCertificateStore, LocalCaIdentity, generate_local_certificates,
    renew_local_leaf_certificate,
};
use std::path::PathBuf;
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
