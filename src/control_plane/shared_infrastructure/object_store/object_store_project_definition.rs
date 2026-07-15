use super::ObjectStorePlanError;
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use serde_json::json;
use std::fmt::{Debug, Formatter};

const MAXIMUM_BUCKET_BYTES: usize = 63;

/// One project bucket, application identity, and bucket-scoped policy.
pub(crate) struct ObjectStoreProjectDefinition {
    bucket: String,
    username: String,
    policy_name: String,
    policy_json: String,
    secret: CredentialSecret,
}

impl ObjectStoreProjectDefinition {
    pub(crate) fn new(
        project_id: &str,
        service_id: &str,
        secret: CredentialSecret,
    ) -> Result<Self, ObjectStorePlanError> {
        let project_id = DnsLabel::new("project", project_id)
            .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
        let service_id = DnsLabel::new("service", service_id)
            .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
        if secret.expose().is_empty() || secret.expose().contains('\0') {
            return Err(ObjectStorePlanError::new(
                "object-store project secret must be non-empty and contain no NUL bytes",
            ));
        }
        let identity = format!("{}-{}", project_id.as_str(), service_id.as_str());
        let bucket = format!("stackctl-{identity}");
        if bucket.len() > MAXIMUM_BUCKET_BYTES {
            return Err(ObjectStorePlanError::new(format!(
                "object-store bucket '{bucket}' exceeds {MAXIMUM_BUCKET_BYTES} bytes"
            )));
        }
        let username = format!("st_{}", identity.replace('-', "_"));
        let policy_name = bucket.clone();
        let bucket_arn = format!("arn:aws:s3:::{bucket}");
        let object_arn = format!("{bucket_arn}/*");
        let policy_json = serde_json::to_string(&json!({
            "Version": "2012-10-17",
            "Statement": [
                {
                    "Effect": "Allow",
                    "Action": [
                        "s3:GetBucketLocation",
                        "s3:GetBucketVersioning",
                        "s3:ListBucket",
                    ],
                    "Resource": [bucket_arn],
                },
                {
                    "Effect": "Allow",
                    "Action": [
                        "s3:AbortMultipartUpload",
                        "s3:DeleteObject",
                        "s3:GetObject",
                        "s3:ListMultipartUploadParts",
                        "s3:PutObject",
                    ],
                    "Resource": [object_arn],
                },
            ],
        }))
        .map_err(|error| {
            ObjectStorePlanError::new(format!("failed to encode object-store policy: {error}"))
        })?;

        Ok(Self {
            bucket,
            username,
            policy_name,
            policy_json,
            secret,
        })
    }

    pub(crate) fn bucket(&self) -> &str {
        &self.bucket
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn policy_name(&self) -> &str {
        &self.policy_name
    }

    pub(crate) fn policy_json(&self) -> &str {
        &self.policy_json
    }

    pub(super) const fn secret(&self) -> &CredentialSecret {
        &self.secret
    }
}

impl Debug for ObjectStoreProjectDefinition {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ObjectStoreProjectDefinition")
            .field("bucket", &self.bucket)
            .field("username", &self.username)
            .field("policy_name", &self.policy_name)
            .field("policy_json", &self.policy_json)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}
