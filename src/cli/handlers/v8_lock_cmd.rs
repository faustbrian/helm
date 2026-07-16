//! Strict v8 YAML artifact-lock commands through singleton-daemon IPC.

use super::log;
use super::v8_project::resolve_v8_project;
use crate::cli::args::{Cli, Commands, LockCommands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    ArtifactLock, ArtifactLockImage, IpcOutcome, IpcPayload, IpcRequest, IpcResult,
    MAX_PROJECT_CONFIG_BYTES, PRESET_ARTIFACT_CATALOG_REVISION, apply_artifact_lock,
    artifact_source, default_unix_daemon_runtime_directory, parse_artifact_lock,
    read_bounded_yaml_file, resolve_preset_artifact, send_unix_request,
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
            let lock = generate_lock(project.config(), |references| {
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
    resolve: Resolve,
) -> Result<ArtifactLock>
where
    Resolve: FnOnce(&BTreeMap<String, String>) -> Result<BTreeMap<String, String>>,
{
    let mut images = BTreeMap::new();
    let mut lock_sources = BTreeMap::new();
    let mut mutable_references = BTreeMap::new();
    let mut uses_catalog = false;

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
                lock_sources.insert(service_id.clone(), source);
                mutable_references.insert(service_id.clone(), image.to_owned());
            }
            continue;
        }

        let preset = service
            .preset()
            .with_context(|| format!("service '{service_id}' has neither an image nor a preset"))?;
        let Some(artifact) = resolve_preset_artifact(preset, service.version())? else {
            continue;
        };
        uses_catalog = true;
        lock_sources.insert(service_id.clone(), source);
        mutable_references.insert(service_id.clone(), artifact.reference().to_owned());
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
            let source = lock_sources.get(&service_id).cloned().with_context(|| {
                format!("daemon returned unrequested service key '{service_id}'")
            })?;
            images.insert(service_id, ArtifactLockImage::new(source, resolved));
        }
    }

    let lock = ArtifactLock::new(images);
    Ok(if uses_catalog {
        lock.with_catalog_revision(PRESET_ARTIFACT_CATALOG_REVISION)
    } else {
        lock
    })
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
    let source =
        read_bounded_yaml_file(lock_path, MAX_PROJECT_CONFIG_BYTES).with_context(|| {
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

    let uses_catalog = config.services().values().any(|service| {
        service.image().is_none()
            && service
                .preset()
                .and_then(|preset| resolve_preset_artifact(preset, service.version()).ok())
                .flatten()
                .is_some()
    });
    if uses_catalog && actual.catalog_revision() != Some(PRESET_ARTIFACT_CATALOG_REVISION) {
        println!(
            "~ catalog_revision {} (was {})",
            PRESET_ARTIFACT_CATALOG_REVISION,
            actual.catalog_revision().unwrap_or("missing")
        );
        changed = true;
    }

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
        .filter_map(|(service_id, service)| {
            let source = match artifact_source(service) {
                Some(source) => source,
                None => {
                    return Some(Err(anyhow::anyhow!(
                        "service '{service_id}' has no lockable artifact source"
                    )));
                }
            };
            if service.image().is_none() {
                let Some(preset) = service.preset() else {
                    return Some(Err(anyhow::anyhow!(
                        "service '{service_id}' has neither an image nor a preset"
                    )));
                };
                match resolve_preset_artifact(preset, service.version()) {
                    Ok(Some(_)) => {}
                    Ok(None) => return None,
                    Err(error) => return Some(Err(error.into())),
                }
            }

            Some(Ok((service_id.clone(), source)))
        })
        .collect()
}

fn load_optional_lock(lock_path: &Path) -> Result<Option<ArtifactLock>> {
    match read_bounded_yaml_file(lock_path, MAX_PROJECT_CONFIG_BYTES) {
        Ok(source) => parse_artifact_lock(&source, lock_path)
            .map(Some)
            .map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error)
            .with_context(|| format!("failed to read artifact lock {}", lock_path.display())),
    }
}

fn publish_lock(path: &Path, lock: &ArtifactLock) -> Result<()> {
    let parent = path
        .parent()
        .context("artifact lock path has no parent directory")?;
    let directory_lock = fs::File::open(parent).with_context(|| {
        format!(
            "failed to open artifact lock directory {}",
            parent.display()
        )
    })?;
    directory_lock.lock().with_context(|| {
        format!(
            "failed to lock artifact lock directory {}",
            parent.display()
        )
    })?;
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!(
            "refusing to replace symbolic-link artifact lock {}",
            path.display()
        );
    }
    let yaml = serde_yaml_ng::to_string(lock).context("failed to serialize v8 artifact lock")?;
    parse_artifact_lock(&yaml, path).context("generated artifact lock failed validation")?;
    let temporary = temporary_path(path);
    match fs::remove_file(&temporary) {
        Ok(()) => fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| {
                format!(
                    "failed to sync artifact lock directory {}",
                    parent.display()
                )
            })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to remove stale temporary lock {}",
                    temporary.display()
                )
            });
        }
    }
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
    path.with_extension("yaml.tmp")
}

#[cfg(test)]
mod tests {
    use super::{generate_lock, publish_lock};
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

        let lock = generate_lock(&config, |references| {
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
        })
        .expect("generated lock");

        assert_eq!(lock.images()["app"].source(), "ghcr.io/stackctl/php:8.4");
        assert!(lock.images()["app"].resolved().contains("@sha256:"));
    }

    #[test]
    fn preset_only_generation_uses_the_versioned_catalog_reference() {
        let config = parse_project_config(
            "schema_version: 8\nservices:\n  db:\n    preset: postgres\n    version: \"17\"\n",
            Path::new("/work/bill/.stackctl.yaml"),
        )
        .expect("project config");

        let lock = generate_lock(&config, |references| {
            assert_eq!(
                references,
                &BTreeMap::from([("db".to_owned(), "postgres:17".to_owned())])
            );
            Ok(BTreeMap::from([(
                "db".to_owned(),
                concat!(
                    "postgres@sha256:",
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                )
                .to_owned(),
            )]))
        })
        .expect("catalog-backed lock");

        assert_eq!(lock.catalog_revision(), Some("2026-07-15.3"));
        assert_eq!(lock.images()["db"].source(), "preset:postgres:17");
    }

    #[test]
    fn application_process_presets_inherit_the_application_artifact() {
        let config = parse_project_config(
            concat!(
                "schema_version: 8\nservices:\n",
                "  app:\n    image: ghcr.io/stackctl/php:8.5\n",
                "  worker:\n    preset: queue-worker\n",
                "  scheduler:\n    preset: scheduler\n"
            ),
            Path::new("/work/bill/.stackctl.yaml"),
        )
        .expect("project config");

        let lock = generate_lock(&config, |references| {
            assert_eq!(
                references,
                &BTreeMap::from([("app".to_owned(), "ghcr.io/stackctl/php:8.5".to_owned(),)])
            );
            Ok(BTreeMap::from([(
                "app".to_owned(),
                concat!(
                    "ghcr.io/stackctl/php@sha256:",
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                )
                .to_owned(),
            )]))
        })
        .expect("application lock");

        assert_eq!(lock.images().len(), 1);
        assert!(lock.images().contains_key("app"));
        assert_eq!(lock.catalog_revision(), None);
    }

    #[test]
    fn publication_recovers_one_stable_interrupted_lock_staging_file() {
        let root = std::env::temp_dir().join(format!(
            "stackctl-artifact-lock-staging-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&root).expect("create project directory");
        let path = root.join(".stackctl.lock.yaml");
        let pending = root.join(".stackctl.lock.yaml.tmp");
        std::fs::write(&pending, "interrupted").expect("write interrupted staging file");
        let config = parse_project_config(
            "schema_version: 8\nservices:\n  app:\n    image: ghcr.io/stackctl/php:8.4\n",
            &root.join(".stackctl.yaml"),
        )
        .expect("project config");
        let lock = generate_lock(&config, |_| {
            Ok(BTreeMap::from([(
                "app".to_owned(),
                concat!(
                    "ghcr.io/stackctl/php@sha256:",
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                )
                .to_owned(),
            )]))
        })
        .expect("artifact lock");

        publish_lock(&path, &lock).expect("publish artifact lock");

        assert!(path.is_file());
        assert!(!pending.exists());

        std::fs::remove_dir_all(root).expect("remove lock fixture");
    }
}
