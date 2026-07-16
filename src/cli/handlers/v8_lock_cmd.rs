//! Strict v8 YAML artifact-lock commands through singleton-daemon IPC.

mod resolve_image_references;

use super::log;
use super::v8_project::resolve_v8_project;
use crate::cli::args::{Cli, Commands, LockCommands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    ArtifactLock, IpcOutcome, IpcResult, MAX_PROJECT_CONFIG_BYTES,
    PRESET_ARTIFACT_CATALOG_REVISION, apply_artifact_lock, artifact_source, generate_artifact_lock,
    parse_artifact_lock, read_bounded_yaml_file, replace_artifact_lock, resolve_preset_artifact,
};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use resolve_image_references::resolve_image_references;

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
            let lock = generate_artifact_lock(project.config(), |references| {
                resolve_image_references(references).map_err(|error| error.to_string())
            })
            .map_err(anyhow::Error::msg)?;
            replace_artifact_lock(&lock_path, &lock)?;
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

fn next_request_id() -> String {
    format!(
        "artifact-lock-{}-{}",
        std::process::id(),
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use crate::control_plane::{
        generate_artifact_lock, parse_project_config, replace_artifact_lock,
    };
    use std::collections::BTreeMap;
    use std::path::Path;

    #[test]
    fn mutable_images_are_resolved_by_exact_service_key() {
        let config = parse_project_config(
            "schema_version: 8\nservices:\n  app:\n    image: ghcr.io/stackctl/php:8.4\n",
            Path::new("/work/bill/.stackctl.yaml"),
        )
        .expect("project config");

        let lock = generate_artifact_lock(&config, |references| {
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

        let lock = generate_artifact_lock(&config, |references| {
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

        let lock = generate_artifact_lock(&config, |references| {
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
        let lock = generate_artifact_lock(&config, |_| {
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

        replace_artifact_lock(&path, &lock).expect("publish artifact lock");

        assert!(path.is_file());
        assert!(!pending.exists());

        std::fs::remove_dir_all(root).expect("remove lock fixture");
    }

    #[cfg(unix)]
    #[test]
    fn publication_refuses_a_symbolic_link_destination() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "stackctl-artifact-lock-symlink-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&root).expect("project directory");
        let victim = root.join("victim.yaml");
        std::fs::write(&victim, "owner data\n").expect("victim");
        let path = root.join(".stackctl.lock.yaml");
        symlink(&victim, &path).expect("artifact lock symlink");
        let lock = crate::control_plane::ArtifactLock::new(BTreeMap::new());

        let error = replace_artifact_lock(&path, &lock).expect_err("symlink refusal");

        assert!(error.to_string().contains("symbolic-link artifact lock"));
        assert_eq!(
            std::fs::read_to_string(victim).expect("victim retained"),
            "owner data\n"
        );
        std::fs::remove_dir_all(root).expect("remove lock fixture");
    }
}
