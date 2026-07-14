use super::{PresetArtifact, PresetArtifactError};

/// Revision binding preset-only lock entries to this exact catalog.
pub(crate) const PRESET_ARTIFACT_CATALOG_REVISION: &str = "2026-07-15.1";

/// Resolves one preset into a deliberate versioned registry source.
pub(crate) fn resolve_preset_artifact(
    preset: &str,
    requested_version: Option<&str>,
) -> Result<Option<PresetArtifact>, PresetArtifactError> {
    let preset = canonical_preset(preset);
    if matches!(preset, "horizon" | "queue-worker" | "queue" | "scheduler") {
        return Ok(None);
    }

    let default = default_version(preset).ok_or_else(|| {
        PresetArtifactError::new(format!(
            "preset '{preset}' has no built-in artifact catalog entry"
        ))
    })?;
    let version = requested_version.unwrap_or(default);
    if version.is_empty() {
        return Err(unsupported(preset, version));
    }

    let reference = match preset {
        "mongodb" => format!("mongo:{version}"),
        "postgres" => format!("postgres:{version}"),
        "mysql" => format!("mysql:{version}"),
        "mariadb" => format!("mariadb:{version}"),
        "sqlserver" => fixed(
            preset,
            version,
            default,
            "mcr.microsoft.com/mssql/server:2022-CU25-ubuntu-22.04",
        )?,
        "redis" => format!("redis:{version}-alpine"),
        "valkey" => format!("valkey/valkey:{version}"),
        "localstack" => format!("localstack/localstack:{version}"),
        "gotenberg" => format!("gotenberg/gotenberg:{version}"),
        "rabbitmq" => format!("rabbitmq:{version}-management"),
        "frankenphp" | "laravel" | "reverb" => {
            format!("ghcr.io/faustbrian/stackctl-php:{version}")
        }
        "dragonfly" => fixed(
            preset,
            version,
            default,
            "docker.dragonflydb.io/dragonflydb/dragonfly:v1.39.0",
        )?,
        "memcached" => fixed(preset, version, default, "memcached:1.6-alpine")?,
        "minio" => fixed(
            preset,
            version,
            default,
            "minio/minio:RELEASE.2025-09-07T16-13-09Z",
        )?,
        "garage" => fixed(preset, version, default, "dxflrs/garage:v2.1.0")?,
        "rustfs" => fixed(preset, version, default, "rustfs/rustfs:1.0.0-beta.2")?,
        "opensearch" => fixed(
            preset,
            version,
            default,
            "opensearchproject/opensearch:3.6.0",
        )?,
        "elasticsearch" => fixed(
            preset,
            version,
            default,
            "docker.elastic.co/elasticsearch/elasticsearch:9.4.2",
        )?,
        "meilisearch" => fixed(preset, version, default, "getmeili/meilisearch:v1.45.1")?,
        "typesense" => fixed(preset, version, default, "typesense/typesense:0.26.0")?,
        "dusk" | "selenium" => fixed(
            preset,
            version,
            default,
            "selenium/standalone-chromium:4.43.0-20260404",
        )?,
        "mailpit" => fixed(preset, version, default, "axllent/mailpit:v1.30.0")?,
        "soketi" => fixed(
            preset,
            version,
            default,
            "quay.io/soketi/soketi:5d188786beaf683aca2115a6247dcdc15c29ac77-16-debian",
        )?,
        _ => return Err(unsupported(preset, version)),
    };

    Ok(Some(PresetArtifact::new(version, reference)))
}

fn canonical_preset(preset: &str) -> &str {
    match preset {
        "pg" | "pgsql" => "postgres",
        "mssql" => "sqlserver",
        other => other,
    }
}

fn default_version(preset: &str) -> Option<&'static str> {
    match preset {
        "mongodb" => Some("8"),
        "postgres" => Some("18"),
        "mysql" => Some("8"),
        "mariadb" => Some("11"),
        "sqlserver" => Some("2022"),
        "redis" => Some("7"),
        "valkey" => Some("8"),
        "dragonfly" | "memcached" | "minio" | "rustfs" | "meilisearch" | "mailpit" | "soketi" => {
            Some("1")
        }
        "garage" => Some("2"),
        "localstack" => Some("4"),
        "opensearch" => Some("3"),
        "elasticsearch" => Some("9"),
        "typesense" => Some("0"),
        "frankenphp" | "laravel" | "reverb" => Some("8.5"),
        "dusk" | "selenium" => Some("4"),
        "gotenberg" => Some("8"),
        "rabbitmq" => Some("3"),
        _ => None,
    }
}

fn fixed(
    preset: &str,
    requested: &str,
    supported: &str,
    reference: &str,
) -> Result<String, PresetArtifactError> {
    if requested != supported {
        return Err(unsupported(preset, requested));
    }

    Ok(reference.to_owned())
}

fn unsupported(preset: &str, version: &str) -> PresetArtifactError {
    PresetArtifactError::new(format!(
        "preset '{preset}' version '{version}' has no exact built-in artifact catalog entry"
    ))
}
