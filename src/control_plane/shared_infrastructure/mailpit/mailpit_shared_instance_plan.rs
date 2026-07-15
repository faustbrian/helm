use super::{MailpitPlanError, MailpitSharedInstancePlanOptions};
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerHealthCheck, ContainerRestartPolicy,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, ResourceKind, RetentionClass,
    VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan, shared_container_name,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::Duration;

const AUTHENTICATION_MOUNT_TARGET: &str = "/etc/stackctl/mailpit";
const PASSWORD_FILE: &str = "/etc/stackctl/mailpit/smtp-passwords";
const DATA_MOUNT_TARGET: &str = "/data";
const SMTP_PORT: u16 = 1025;
const UI_PORT: u16 = 8025;

/// Exact Engine resources for one attributed shared Mailpit instance.
pub(crate) struct MailpitSharedInstancePlan {
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    authentication_revision: String,
}

impl MailpitSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: MailpitSharedInstancePlanOptions,
    ) -> Result<Self, MailpitPlanError> {
        let profile = shared.profile();
        if profile.implementation() != "mailpit" {
            return Err(MailpitPlanError::new(format!(
                "Mailpit plan cannot materialize implementation '{}'",
                profile.implementation()
            )));
        }
        if profile.isolation() != IsolationCapability::None {
            return Err(MailpitPlanError::new(
                "Mailpit attribution requires the explicit no-isolation capability",
            ));
        }
        if !options.authentication_directory.is_absolute() {
            return Err(MailpitPlanError::new(format!(
                "Mailpit authentication directory '{}' must be absolute",
                options.authentication_directory.display()
            )));
        }
        let authentication_directory =
            options.authentication_directory.to_str().ok_or_else(|| {
                MailpitPlanError::new(
                    "Mailpit authentication directory must be valid UTF-8 for the Engine API",
                )
            })?;
        validate_revision("authentication", &options.authentication_revision)?;
        let platform = profile.platform_architecture().ok_or_else(|| {
            MailpitPlanError::new("Mailpit compatibility profile requires a Linux platform")
        })?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint
            .strip_prefix("sha256:")
            .ok_or_else(|| MailpitPlanError::new("Mailpit fingerprint is malformed"))?;
        let container_name = shared_container_name(identity);
        let volume_name = format!("{container_name}-data");
        let retention = match profile.persistence() {
            PersistenceMode::Persistent => RetentionClass::Persistent,
            PersistenceMode::Ephemeral => RetentionClass::Disposable,
        };
        let effective_revision =
            effective_revision(&options.desired_revision, &options.authentication_revision);
        let container_metadata = metadata(
            &options,
            ResourceKind::SharedService,
            retention,
            fingerprint,
            &effective_revision,
        )?
        .with_resource_id(&container_name)
        .and_then(|metadata| {
            metadata.with_compatibility_profile(profile.implementation(), profile.major_version())
        })
        .map_err(|error| MailpitPlanError::new(error.to_string()))?;
        let authentication_mount =
            BindMount::read_only(authentication_directory, AUTHENTICATION_MOUNT_TARGET)
                .map_err(|error| MailpitPlanError::new(error.to_string()))?;
        let health_check = ContainerHealthCheck::new(
            vec!["/mailpit".to_owned(), "readyz".to_owned()],
            Duration::from_secs(15),
            Duration::from_secs(5),
            Duration::from_secs(10),
            4,
        )
        .map_err(|error| MailpitPlanError::new(error.to_string()))?;
        let mut container = ContainerCreateOptions::new(
            &container_name,
            profile.image_digest(),
            container_metadata,
        )
        .and_then(|request| request.with_network(&options.network_name))
        .and_then(|request| request.with_platform(platform))
        .and_then(|request| {
            request.with_environment(BTreeMap::from([
                ("MP_DATABASE".to_owned(), "/data/mailpit.db".to_owned()),
                ("MP_SMTP_AUTH_FILE".to_owned(), PASSWORD_FILE.to_owned()),
                ("MP_SMTP_AUTH_ALLOW_INSECURE".to_owned(), "true".to_owned()),
                ("MP_TAGS_USERNAME".to_owned(), "true".to_owned()),
            ]))
        })
        .map_err(|error| MailpitPlanError::new(error.to_string()))?
        .with_bind_mount(authentication_mount)
        .with_health_check(health_check)
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped);
        let volume = if profile.persistence() == PersistenceMode::Persistent {
            let volume_metadata = metadata(
                &options,
                ResourceKind::Volume,
                retention,
                fingerprint,
                &effective_revision,
            )?
            .with_resource_id(&container_name)
            .map_err(|error| MailpitPlanError::new(error.to_string()))?;
            let volume = VolumeCreateOptions::new(&volume_name, volume_metadata)
                .map_err(|error| MailpitPlanError::new(error.to_string()))?;
            let mount = VolumeMount::read_write(&volume_name, DATA_MOUNT_TARGET)
                .map_err(|error| MailpitPlanError::new(error.to_string()))?;
            container = container.with_volume_mount(mount);
            Some(volume)
        } else {
            None
        };

        Ok(Self {
            container,
            volume,
            authentication_revision: options.authentication_revision,
        })
    }

    pub(crate) const fn container(&self) -> &ContainerCreateOptions {
        &self.container
    }

    pub(crate) const fn volume(&self) -> Option<&VolumeCreateOptions> {
        self.volume.as_ref()
    }

    pub(crate) fn authentication_revision(&self) -> &str {
        &self.authentication_revision
    }

    pub(crate) const fn authentication_mount_target(&self) -> &'static str {
        AUTHENTICATION_MOUNT_TARGET
    }

    pub(crate) const fn smtp_port(&self) -> u16 {
        SMTP_PORT
    }

    pub(crate) const fn ui_port(&self) -> u16 {
        UI_PORT
    }
}

fn effective_revision(desired_revision: &str, authentication_revision: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"stackctl-mailpit-revision\0");
    hasher.update(desired_revision.as_bytes());
    hasher.update(b"\0");
    hasher.update(authentication_revision.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn validate_revision(kind: &str, revision: &str) -> Result<(), MailpitPlanError> {
    let digest = revision.strip_prefix("sha256:").unwrap_or_default();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MailpitPlanError::new(format!(
            "Mailpit {kind} revision must be a sha256 digest"
        )));
    }
    Ok(())
}

fn metadata(
    options: &MailpitSharedInstancePlanOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
    desired_revision: &str,
) -> Result<ManagedResourceMetadata, MailpitPlanError> {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: None,
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: desired_revision.to_owned(),
        retention,
    })
    .map_err(|error| MailpitPlanError::new(error.to_string()))
}
