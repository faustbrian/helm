//! Strict v8 YAML artifact-lock commands through singleton-daemon IPC.

use super::log;
use super::v8_project::resolve_v8_project;
use crate::cli::args::{Cli, Commands, LockCommands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    ArtifactLock, ArtifactLockImage, IpcOutcome, IpcPayload, IpcRequest, IpcResult,
    apply_artifact_lock, artifact_source, default_unix_daemon_runtime_directory,
    parse_artifact_lock, send_unix_request,
};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);
const ENGINE_CONNECT_TIMEOUT: Duration = Duration::from_secs(60);
const ENGINE_RETRY_INTERVAL: Duration = Duration::from_millis(250);
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn handle_v8_lock(cli: &Cli, context: &CliDispatchContext<'_>) -> Result<bool> {
    let Commands::Lock(args) = &cli.command else {
        return Ok(false);
    };
    let Some(project) = resolve_v8_project(context)? else {
        return Ok(false);
    };
    let lock_path = project.root().join(".stackctl.lock.yaml");

    match &args.command {
        LockCommands::Images => {
            if context.dry_run() {
                bail!("--dry-run is not supported when publishing a v8 artifact lock");
            }
            let lock = generate_lock(project.config(), &lock_path, |references| {
                resolve_through_daemon(
                    &default_unix_daemon_runtime_directory()?.join("daemon.sock"),
                    references,
                )
            })?;
            publish_lock(&lock_path, &lock)?;
            log::info_if_not_quiet(
                context.quiet(),
                "lock",
                &format!(
                    "Wrote {} with {} image entries",
                    lock_path.display(),
                    lock.images().len()
                ),
            );
        }
        LockCommands::Verify => {
            verify_lock(project.config(), &lock_path)?;
            log::info_if_not_quiet(context.quiet(), "lock", "Artifact lock is in sync");
        }
        LockCommands::Diff => print_diff(project.config(), &lock_path)?,
    }

    Ok(true)
}

fn generate_lock<Resolve>(
    config: &crate::control_plane::RawProjectConfig,
    lock_path: &Path,
    resolve: Resolve,
) -> Result<ArtifactLock>
where
    Resolve: FnOnce(&BTreeMap<String, String>) -> Result<BTreeMap<String, String>>,
{
    let existing = load_optional_lock(lock_path).ok().flatten();
    let mut images = BTreeMap::new();
    let mut mutable_references = BTreeMap::new();

    for (service_id, service) in config.services() {
        let source = artifact_source(service)
            .with_context(|| format!("service '{service_id}' has no lockable artifact source"))?;
        if let Some(image) = service.image() {
            if is_immutable_registry_reference(image) {
                images.insert(
                    service_id.clone(),
                    ArtifactLockImage::new(source, image.to_owned()),
                );
            } else {
                mutable_references.insert(service_id.clone(), image.to_owned());
            }
            continue;
        }

        let Some(existing_image) = existing
            .as_ref()
            .and_then(|lock| lock.images().get(service_id))
            .filter(|entry| entry.source() == source)
            .filter(|entry| is_immutable_registry_reference(entry.resolved()))
        else {
            bail!(
                "service '{service_id}' uses preset source '{source}', but no exact built-in image catalog resolution exists yet; declare an explicit image or retain a matching lock entry"
            );
        };
        images.insert(
            service_id.clone(),
            ArtifactLockImage::new(source, existing_image.resolved().to_owned()),
        );
    }

    if !mutable_references.is_empty() {
        let resolved = resolve(&mutable_references)?;
        if resolved.keys().ne(mutable_references.keys()) {
            bail!("daemon returned a different image-reference key set than requested");
        }
        for (service_id, resolved) in resolved {
            if !is_immutable_registry_reference(&resolved) {
                bail!("daemon returned a mutable image resolution for service '{service_id}'");
            }
            let source = mutable_references
                .get(&service_id)
                .expect("validated response key remains requested")
                .clone();
            images.insert(service_id, ArtifactLockImage::new(source, resolved));
        }
    }

    Ok(ArtifactLock::new(images))
}

fn resolve_through_daemon(
    socket_path: &Path,
    references: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let deadline = Instant::now() + ENGINE_CONNECT_TIMEOUT;
    loop {
        let response = send_unix_request(
            socket_path,
            &IpcRequest::new(
                next_request_id(),
                IpcPayload::ResolveImageReferences {
                    references: references.clone(),
                },
            ),
            REQUEST_TIMEOUT,
        )?;
        if !engine_is_reconnecting(response.outcome()) || Instant::now() >= deadline {
            return resolved_references(response.outcome());
        }
        std::thread::sleep(ENGINE_RETRY_INTERVAL);
    }
}

fn verify_lock(config: &crate::control_plane::RawProjectConfig, lock_path: &Path) -> Result<()> {
    let source = fs::read_to_string(lock_path).with_context(|| {
        format!(
            "failed to read {}; run `stackctl lock images`",
            lock_path.display()
        )
    })?;
    let lock = parse_artifact_lock(&source, lock_path)?;
    let mut candidate = config.clone();
    apply_artifact_lock(&mut candidate, &lock, lock_path)?;
    let expected = expected_sources(config)?;

    if lock.images().len() != expected.len()
        || expected.iter().any(|(service_id, source)| {
            lock.images()
                .get(service_id)
                .is_none_or(|entry| entry.source() != source)
        })
    {
        bail!("artifact lock is out of sync; run `stackctl lock images`");
    }

    Ok(())
}

fn print_diff(config: &crate::control_plane::RawProjectConfig, lock_path: &Path) -> Result<()> {
    let expected = expected_sources(config)?;
    let actual =
        load_optional_lock(lock_path)?.unwrap_or_else(|| ArtifactLock::new(BTreeMap::new()));
    let mut changed = false;

    for (service_id, source) in &expected {
        match actual.images().get(service_id) {
            None => {
                println!("+ {service_id} {source}");
                changed = true;
            }
            Some(entry) if entry.source() != source => {
                println!("~ {service_id} {source} (was {})", entry.source());
                changed = true;
            }
            Some(_) => {}
        }
    }
    for (service_id, entry) in actual.images() {
        if !expected.contains_key(service_id) {
            println!("- {service_id} {}", entry.source());
            changed = true;
        }
    }
    if !changed {
        println!("No artifact lock changes");
    }

    Ok(())
}

fn expected_sources(
    config: &crate::control_plane::RawProjectConfig,
) -> Result<BTreeMap<String, String>> {
    config
        .services()
        .iter()
        .map(|(service_id, service)| {
            artifact_source(service)
                .map(|source| (service_id.clone(), source))
                .with_context(|| format!("service '{service_id}' has no lockable artifact source"))
        })
        .collect()
}

fn load_optional_lock(lock_path: &Path) -> Result<Option<ArtifactLock>> {
    match fs::read_to_string(lock_path) {
        Ok(source) => parse_artifact_lock(&source, lock_path)
            .map(Some)
            .map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error)
            .with_context(|| format!("failed to read artifact lock {}", lock_path.display())),
    }
}

fn publish_lock(path: &Path, lock: &ArtifactLock) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!(
            "refusing to replace symbolic-link artifact lock {}",
            path.display()
        );
    }
    let yaml = serde_yaml_ng::to_string(lock).context("failed to serialize v8 artifact lock")?;
    parse_artifact_lock(&yaml, path).context("generated artifact lock failed validation")?;
    let temporary = temporary_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("failed to create temporary lock {}", temporary.display()))?;
    let result = (|| -> Result<()> {
        file.write_all(yaml.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
            .with_context(|| format!("failed to publish artifact lock {}", path.display()))?;
        let parent = path
            .parent()
            .context("artifact lock path has no parent directory")?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| {
                format!(
                    "failed to sync artifact lock directory {}",
                    parent.display()
                )
            })?;
        Ok(())
    })();
    if result.is_err() {
        drop(fs::remove_file(&temporary));
    }

    result
}

fn resolved_references(outcome: &IpcOutcome) -> Result<BTreeMap<String, String>> {
    match outcome {
        IpcOutcome::Success {
            result: IpcResult::ImageReferencesResolved { references },
        } => Ok(references.clone()),
        IpcOutcome::Success { .. } => {
            bail!("daemon returned an unexpected image-resolution result")
        }
        IpcOutcome::Failure { diagnostics } => {
            let detail = diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
                .collect::<Vec<_>>()
                .join("; ");
            bail!("daemon image resolution failed: {detail}")
        }
    }
}

fn engine_is_reconnecting(outcome: &IpcOutcome) -> bool {
    matches!(
        outcome,
        IpcOutcome::Failure { diagnostics }
            if !diagnostics.is_empty()
                && diagnostics.iter().all(|diagnostic| {
                    diagnostic.code() == "engine_unavailable" && diagnostic.retryable()
                })
    )
}

fn is_immutable_registry_reference(image: &str) -> bool {
    image
        .rsplit_once("@sha256:")
        .is_some_and(|(repository, digest)| {
            !repository.is_empty()
                && digest.len() == 64
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn next_request_id() -> String {
    format!(
        "artifact-lock-{}-{}",
        std::process::id(),
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn temporary_path(path: &Path) -> PathBuf {
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("yaml.tmp-{}-{sequence}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::generate_lock;
    use crate::control_plane::parse_project_config;
    use std::collections::BTreeMap;
    use std::path::Path;

    #[test]
    fn mutable_images_are_resolved_by_exact_service_key() {
        let config = parse_project_config(
            "schema_version: 8\nservices:\n  app:\n    image: ghcr.io/stackctl/php:8.4\n",
            Path::new("/work/bill/.stackctl.yaml"),
        )
        .expect("project config");

        let lock = generate_lock(
            &config,
            Path::new("/work/bill/.stackctl.lock.yaml"),
            |references| {
                assert_eq!(
                    references,
                    &BTreeMap::from([("app".to_owned(), "ghcr.io/stackctl/php:8.4".to_owned(),)])
                );
                Ok(BTreeMap::from([(
                    "app".to_owned(),
                    concat!(
                        "ghcr.io/stackctl/php@sha256:",
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    )
                    .to_owned(),
                )]))
            },
        )
        .expect("generated lock");

        assert_eq!(lock.images()["app"].source(), "ghcr.io/stackctl/php:8.4");
        assert!(lock.images()["app"].resolved().contains("@sha256:"));
    }

    #[test]
    fn preset_only_generation_never_guesses_an_image() {
        let config = parse_project_config(
            "schema_version: 8\nservices:\n  db:\n    preset: postgres\n    version: \"17\"\n",
            Path::new("/work/bill/.stackctl.yaml"),
        )
        .expect("project config");

        let error = generate_lock(&config, Path::new("/work/bill/.stackctl.lock.yaml"), |_| {
            unreachable!("preset-only generation must fail before Engine access")
        })
        .expect_err("missing built-in catalog");

        assert!(
            error
                .to_string()
                .contains("no exact built-in image catalog")
        );
        assert!(error.to_string().contains("preset:postgres:17"));
    }
}
