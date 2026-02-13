//! Object-store backend env inference (MinIO/RustFS).

use std::collections::HashMap;

use crate::config::ServiceConfig;

use super::super::{insert_if_absent, service_endpoint};

/// Applies inferred S3-compatible env variables.
///
/// Forces path-style endpoint mode because local S3-compatible providers often
/// do not support virtual-host bucket routing.
pub(super) fn apply(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(vars, "FILESYSTEM_DISK", "s3".to_owned());
    insert_if_absent(
        vars,
        "AWS_ACCESS_KEY_ID",
        service
            .access_key
            .clone()
            .unwrap_or_else(|| "minio".to_owned()),
    );
    insert_if_absent(
        vars,
        "AWS_SECRET_ACCESS_KEY",
        service
            .secret_key
            .clone()
            .unwrap_or_else(|| "miniosecret".to_owned()),
    );
    insert_if_absent(
        vars,
        "AWS_DEFAULT_REGION",
        service
            .region
            .clone()
            .unwrap_or_else(|| "us-east-1".to_owned()),
    );
    insert_if_absent(
        vars,
        "AWS_BUCKET",
        service.bucket.clone().unwrap_or_else(|| "app".to_owned()),
    );
    insert_if_absent(vars, "AWS_ENDPOINT", service_endpoint(service));
    insert_if_absent(vars, "AWS_URL", service_endpoint(service));
    insert_if_absent(vars, "AWS_USE_PATH_STYLE_ENDPOINT", "true".to_owned());
}
