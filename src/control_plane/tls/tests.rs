use super::generate_local_certificates;
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
