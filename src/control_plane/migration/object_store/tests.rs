use super::backup_minio_bucket::validate_version_inventory;

#[test]
fn minio_backup_rejects_version_history_it_cannot_preserve() {
    let error = validate_version_inventory(
        "stackctl-bill-files",
        br#"{"status":"success","versioning":{"status":"Enabled"}}"#,
    )
    .expect_err("versioned bucket must fail closed");

    assert!(error.to_string().contains("uses versioning"));
}

#[test]
fn minio_backup_accepts_an_unversioned_inventory() {
    validate_version_inventory(
        "stackctl-bill-files",
        br#"{"status":"success","versioning":{}}"#,
    )
    .expect("unversioned bucket");
}
