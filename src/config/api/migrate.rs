//! Non-destructive v7 configuration migration into a strict v8 YAML candidate.

use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::super::{ContainerEngine, DomainStrategy, Driver, ProjectType, RawServiceConfig};
use super::load_save::{RawConfigPathOptions, load_config_with, load_raw_config_with};
use super::project::{ProjectRootPathOptions, project_root_with};
use super::{ConfigMigrationResult, MigrateConfigOptions, MigrationDifference};

/// Emits a v8 YAML candidate and report without modifying the v7 source.
pub fn migrate_config_with(options: MigrateConfigOptions<'_>) -> Result<ConfigMigrationResult> {
    if options.to != "yaml" {
        anyhow::bail!(
            "unsupported migration target '{}'; v8 supports only `--to yaml`",
            options.to
        );
    }
    let path_options = ProjectRootPathOptions {
        config_path: options.config_path,
        project_root: options.project_root,
        runtime_env: options.runtime_env,
    };
    let project_root = project_root_with(path_options)?;
    let source = source_path(options.config_path, &project_root)?;
    let source_bytes = fs::read(&source)
        .with_context(|| format!("failed to read v7 config '{}'", source.display()))?;
    let raw = load_raw_config_with(RawConfigPathOptions {
        config_path: Some(&source),
        project_root: Some(&project_root),
        runtime_env: options.runtime_env,
    })?;
    let expanded = load_config_with(ProjectRootPathOptions {
        config_path: Some(&source),
        project_root: Some(&project_root),
        runtime_env: options.runtime_env,
    })?;
    if raw.service.len() != expanded.service.len() {
        anyhow::bail!("v7 service expansion changed service cardinality; refusing to guess");
    }

    let mut differences = project_differences(&raw, &project_root);
    let mut services = BTreeMap::new();
    for (index, (raw_service, service)) in raw.service.iter().zip(&expanded.service).enumerate() {
        differences.extend(service_differences(index, raw_service));
        let name = service.name.clone();
        let migrated = MigratedService {
            preset: raw_service
                .preset
                .clone()
                .or_else(|| Some(driver_preset(service.driver).to_owned())),
            image: raw_service.image.clone(),
            php_extensions: raw_service.php_extensions.clone().unwrap_or_default(),
            depends_on: raw_service.depends_on.clone().unwrap_or_default(),
            database: raw_service.database.clone(),
            command: raw_service.command.clone(),
            environment: raw_service
                .env
                .clone()
                .unwrap_or_default()
                .into_iter()
                .collect(),
        };
        if services.insert(name.clone(), migrated).is_some() {
            anyhow::bail!("v7 services resolve to duplicate identity '{name}'");
        }
    }
    let project = migrated_project_name(raw.container_prefix.as_deref(), &project_root);
    let candidate = MigratedProject {
        schema_version: 8,
        project,
        services,
    };
    let yaml = serde_yaml_ng::to_string(&candidate)
        .context("failed to serialize v8 YAML migration candidate")?;
    validate_candidate(&yaml, &project_root)?;
    let candidate_path = candidate_path(&source);
    let report_path = source.with_file_name(".stackctl-migration-report.json");
    preflight_output(&source, &candidate_path, &report_path)?;
    let report = MigrationReport {
        source: source.display().to_string(),
        candidate: candidate_path.display().to_string(),
        source_sha256: format!("sha256:{}", hex::encode(Sha256::digest(&source_bytes))),
        differences: &differences,
    };
    let report_json =
        serde_json::to_vec_pretty(&report).context("failed to serialize v8 migration report")?;
    publish_file(&report_path, &report_json)?;
    if let Err(error) = publish_file(&candidate_path, yaml.as_bytes()) {
        let cleanup = fs::remove_file(&report_path);
        return match cleanup {
            Ok(()) => Err(error),
            Err(cleanup) => Err(error.context(format!(
                "also failed to remove partial migration report '{}': {cleanup}",
                report_path.display()
            ))),
        };
    }

    Ok(ConfigMigrationResult::new(
        source,
        candidate_path,
        report_path,
        differences,
    ))
}

#[derive(Serialize)]
struct MigratedProject {
    schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    services: BTreeMap<String, MigratedService>,
}

#[derive(Serialize)]
struct MigratedService {
    #[serde(skip_serializing_if = "Option::is_none")]
    preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    php_extensions: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    environment: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct MigrationReport<'difference> {
    source: String,
    candidate: String,
    source_sha256: String,
    differences: &'difference [MigrationDifference],
}

fn validate_candidate(yaml: &str, project_root: &Path) -> Result<()> {
    let path = project_root.join(".stackctl.yaml");
    let raw = crate::control_plane::parse_project_config(yaml, &path)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    crate::control_plane::resolve_desired_project(raw, project_root)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    Ok(())
}

fn source_path(explicit: Option<&Path>, project_root: &Path) -> Result<PathBuf> {
    if let Some(explicit) = explicit {
        return Ok(explicit.to_path_buf());
    }
    super::config_io::resolve_config_path(ProjectRootPathOptions::new(None, Some(project_root)))
}

fn candidate_path(source: &Path) -> PathBuf {
    if source.file_name().and_then(|name| name.to_str()) == Some(".stackctl.yaml") {
        source.with_file_name(".stackctl.v8.yaml")
    } else {
        source.with_file_name(".stackctl.yaml")
    }
}

fn preflight_output(source: &Path, candidate: &Path, report: &Path) -> Result<()> {
    if candidate == source {
        anyhow::bail!("migration candidate must not overwrite the v7 source");
    }
    for path in [candidate, report] {
        if path.exists() {
            anyhow::bail!(
                "migration output '{}' already exists; refusing to overwrite it",
                path.display()
            );
        }
    }

    Ok(())
}

fn publish_file(path: &Path, contents: &[u8]) -> Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("migration path '{}' has no parent", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("migration path '{}' is not valid UTF-8", path.display()))?;
    let temporary = directory.join(format!(".{name}-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| {
            format!(
                "failed to create migration output '{}'",
                temporary.display()
            )
        })?;
    file.write_all(contents)
        .with_context(|| format!("failed to write migration output '{}'", temporary.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync migration output '{}'", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to publish migration output '{}'", path.display()))?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .with_context(|| {
            format!(
                "failed to sync migration directory '{}'",
                directory.display()
            )
        })?;

    Ok(())
}

fn migrated_project_name(prefix: Option<&str>, root: &Path) -> Option<String> {
    let directory = root.file_name().and_then(|name| name.to_str());
    prefix
        .filter(|prefix| Some(*prefix) != directory && *prefix != "stackctl")
        .map(str::to_owned)
}

fn project_differences(
    raw: &super::super::RawConfig,
    project_root: &Path,
) -> Vec<MigrationDifference> {
    let mut differences = Vec::new();
    if raw.project_type == Some(ProjectType::Library) {
        differences.push(blocking(
            "project_type",
            "v8 does not yet model library-project runtime behavior",
        ));
    }
    if let Some(engine) = raw.container_engine {
        differences.push(MigrationDifference::new(
            "container_engine",
            format!(
                "moved to daemon state; select {} during v8 initialization",
                engine_name(engine)
            ),
            false,
        ));
    }
    if raw.domain_strategy == Some(DomainStrategy::Random) {
        differences.push(blocking(
            "domain_strategy",
            "random v7 domains cannot be preserved under deterministic v8 naming",
        ));
    }
    if let Some(prefix) = raw.container_prefix.as_deref() {
        let directory = project_root.file_name().and_then(|name| name.to_str());
        if prefix != "stackctl" && Some(prefix) != directory {
            differences.push(MigrationDifference::new(
                "container_prefix",
                "mapped to the explicit v8 project identity; verify resulting domains",
                false,
            ));
        }
    }
    if !raw.swarm.is_empty() {
        differences.push(blocking(
            "swarm",
            "v7 sharing targets require an explicit v8 compatibility migration",
        ));
    }

    differences
}

fn service_differences(index: usize, service: &RawServiceConfig) -> Vec<MigrationDifference> {
    let mut differences = Vec::new();
    let prefix = format!("service[{index}]");
    macro_rules! unsupported_option {
        ($field:ident, $detail:literal) => {
            if service.$field.is_some() {
                differences.push(blocking(
                    format!("{prefix}.{}", stringify!($field)),
                    $detail,
                ));
            }
        };
    }
    unsupported_option!(host, "host binding is replaced by the private v8 network");
    unsupported_option!(port, "host ports are replaced by private service endpoints");
    unsupported_option!(
        username,
        "existing credentials require a resource migration"
    );
    unsupported_option!(
        password,
        "existing credentials require a resource migration"
    );
    unsupported_option!(bucket, "existing buckets require a data migration");
    unsupported_option!(
        access_key,
        "existing credentials require a resource migration"
    );
    unsupported_option!(
        secret_key,
        "existing credentials require a resource migration"
    );
    unsupported_option!(api_key, "existing credentials require a resource migration");
    unsupported_option!(
        region,
        "custom object-store region is not represented in v8 YAML"
    );
    unsupported_option!(
        scheme,
        "custom service scheme is not represented in v8 YAML"
    );
    unsupported_option!(
        domain,
        "v7 domains are replaced by deterministic v8 domains"
    );
    unsupported_option!(
        domains,
        "v7 domains are replaced by deterministic v8 domains"
    );
    unsupported_option!(
        container_port,
        "container port overrides require an adapter"
    );
    unsupported_option!(smtp_port, "SMTP host ports are removed in v8");
    unsupported_option!(volumes, "custom mounts require security review");
    unsupported_option!(
        seed_file,
        "seed files require an explicit project-container hook"
    );
    unsupported_option!(health_path, "custom health checks require an adapter");
    unsupported_option!(health_statuses, "custom health checks require an adapter");
    unsupported_option!(restart, "restart overrides require an explicit v8 policy");
    unsupported_option!(
        localhost_tls,
        "localhost TLS is replaced by the singleton gateway"
    );
    unsupported_option!(octane, "Octane settings require a runtime migration");
    unsupported_option!(
        octane_workers,
        "Octane settings require a runtime migration"
    );
    unsupported_option!(
        octane_max_requests,
        "Octane settings require a runtime migration"
    );
    unsupported_option!(
        trust_container_ca,
        "container-local CA trust is removed in v8"
    );
    unsupported_option!(env_mapping, "environment mappings require explicit review");
    unsupported_option!(
        javascript,
        "JavaScript toolchain settings require runtime migration"
    );
    unsupported_option!(container_name, "container names are daemon-owned in v8");
    if !service.hook.is_empty() {
        differences.push(blocking(
            format!("{prefix}.hook"),
            "hooks require an explicit project-container security review",
        ));
    }

    differences
}

fn blocking(path: impl Into<String>, detail: impl Into<String>) -> MigrationDifference {
    MigrationDifference::new(path, detail, true)
}

fn driver_preset(driver: Driver) -> &'static str {
    match driver {
        Driver::Mongodb => "mongodb",
        Driver::Memcached => "memcached",
        Driver::Postgres => "postgres",
        Driver::Mysql => "mysql",
        Driver::Sqlserver => "sqlserver",
        Driver::Redis => "redis",
        Driver::Valkey => "valkey",
        Driver::Dragonfly => "dragonfly",
        Driver::Minio => "minio",
        Driver::Garage => "garage",
        Driver::Rustfs => "rustfs",
        Driver::Localstack => "localstack",
        Driver::Opensearch => "opensearch",
        Driver::Elasticsearch => "elasticsearch",
        Driver::Meilisearch => "meilisearch",
        Driver::Typesense => "typesense",
        Driver::Frankenphp => "frankenphp",
        Driver::Reverb => "reverb",
        Driver::Horizon => "horizon",
        Driver::Scheduler => "scheduler",
        Driver::Dusk => "dusk",
        Driver::Gotenberg => "gotenberg",
        Driver::Mailhog => "mailhog",
        Driver::Rabbitmq => "rabbitmq",
        Driver::Soketi => "soketi",
    }
}

const fn engine_name(engine: ContainerEngine) -> &'static str {
    match engine {
        ContainerEngine::Docker => "Docker",
        ContainerEngine::Podman => "Podman",
    }
}

#[cfg(test)]
mod tests {
    use super::{MigrateConfigOptions, migrate_config_with};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "stackctl-v8-config-migrate-{name}-{}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create migration root");
        root
    }

    fn write_v7(root: &Path, source: &str) -> PathBuf {
        let path = root.join(".stackctl.toml");
        fs::write(&path, source).expect("write v7 config");
        path
    }

    #[test]
    fn emits_valid_yaml_and_report_without_modifying_v7_source() {
        let root = root("safe");
        let source = r#"project_type = "project"
container_prefix = "stackctl"

[[service]]
name = "app"
preset = "laravel"
image = "ghcr.io/stackctl/php:8.4"
php_extensions = ["intl"]
depends_on = ["database"]

[[service]]
name = "database"
preset = "postgres"
database = "bill"
"#;
        let path = write_v7(&root, source);

        let result = migrate_config_with(MigrateConfigOptions {
            config_path: Some(&path),
            project_root: Some(&root),
            runtime_env: None,
            to: "yaml",
        })
        .expect("migrate v7 config");

        assert_eq!(fs::read_to_string(&path).expect("v7 source"), source);
        let yaml = fs::read_to_string(result.candidate()).expect("v8 candidate");
        assert!(yaml.contains("schema_version: 8"));
        assert!(yaml.contains("services:"));
        assert!(result.report().is_file());
        assert!(!result.has_blocking_differences());
        crate::control_plane::parse_project_config(&yaml, result.candidate())
            .expect("strict v8 candidate");

        fs::remove_dir_all(root).expect("remove migration root");
    }

    #[test]
    fn reports_unsupported_secrets_without_copying_values() {
        let root = root("secret");
        let path = write_v7(
            &root,
            r#"project_type = "project"
container_prefix = "stackctl"
[[service]]
name = "database"
preset = "postgres"
password = "legacy-project-secret"
"#,
        );

        let result = migrate_config_with(MigrateConfigOptions {
            config_path: Some(&path),
            project_root: Some(&root),
            runtime_env: None,
            to: "yaml",
        })
        .expect("emit review candidate");
        let yaml = fs::read_to_string(result.candidate()).expect("candidate");
        let report = fs::read_to_string(result.report()).expect("report");

        assert!(result.has_blocking_differences());
        assert!(result.differences().iter().any(|difference| {
            difference.path().ends_with(".password") && difference.blocking()
        }));
        assert!(!yaml.contains("legacy-project-secret"));
        assert!(!report.contains("legacy-project-secret"));

        fs::remove_dir_all(root).expect("remove migration root");
    }

    #[test]
    fn refuses_to_overwrite_an_existing_candidate() {
        let root = root("collision");
        let path = write_v7(
            &root,
            "project_type = \"project\"\ncontainer_prefix = \"stackctl\"\n[[service]]\nname = \"app\"\npreset = \"laravel\"\n",
        );
        let candidate = root.join(".stackctl.yaml");
        fs::write(&candidate, "owned by user\n").expect("existing candidate");

        let error = migrate_config_with(MigrateConfigOptions {
            config_path: Some(&path),
            project_root: Some(&root),
            runtime_env: None,
            to: "yaml",
        })
        .expect_err("candidate collision");

        assert!(error.to_string().contains("refusing to overwrite"));
        assert_eq!(
            fs::read_to_string(candidate).expect("existing candidate"),
            "owned by user\n"
        );

        fs::remove_dir_all(root).expect("remove migration root");
    }
}
