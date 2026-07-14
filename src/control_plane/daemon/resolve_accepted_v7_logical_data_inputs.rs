use super::ipc::IpcV7ProjectInventory;
use super::{AcceptedV7LogicalDataInput, V7LogicalDataCredential};
use crate::config::{Config, Driver, Kind, LoadConfigPathOptions, ServiceConfig, load_config_with};
use crate::control_plane::migration::{
    V7LogicalDataMigrationSource, V7LogicalDataMigrationSourceOptions, V7MigrationServiceAdapter,
    V7MinioCredential, V7MongoDbCredential, V7MySqlCredential, V7PostgresCredential,
    V7RabbitMqCredential, V7RedisCredential, V7SqlServerCredential,
};
use crate::control_plane::state::AcceptedV7InventoryRecord;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Reopens unchanged v7 config and binds typed credentials to accepted sources.
pub(crate) fn resolve_accepted_v7_logical_data_inputs(
    accepted: &AcceptedV7InventoryRecord,
    maximum_config_bytes: usize,
) -> Result<Vec<AcceptedV7LogicalDataInput>, String> {
    let adapter_plan = super::select_accepted_v7_migration_adapters(accepted)?;
    let logical_selections = adapter_plan
        .services()
        .iter()
        .filter(|selection| logical_adapter(selection.adapter()))
        .collect::<Vec<_>>();
    if logical_selections.is_empty() {
        return Ok(Vec::new());
    }

    let config = load_exact_config(accepted, maximum_config_bytes)?;
    let inventory = serde_json::from_str::<IpcV7ProjectInventory>(accepted.inventory_json())
        .map_err(|error| format!("accepted v7 logical input is invalid: {error}"))?;
    if inventory.project_id() != accepted.project_id()
        || inventory.canonical_project_path() != accepted.canonical_project_path()
        || inventory.source_revision() != accepted.source_revision()
    {
        return Err("accepted v7 logical input identity is inconsistent".to_owned());
    }
    let mut inputs = Vec::new();
    for selection in logical_selections {
        let service = one(
            inventory
                .services()
                .iter()
                .filter(|service| service.service_id() == selection.service_id())
                .collect(),
            &format!("accepted service '{}'", selection.service_id()),
        )?;
        let configured = one(
            config
                .service
                .iter()
                .filter(|service| service.name == selection.service_id())
                .collect(),
            &format!("legacy config service '{}'", selection.service_id()),
        )?;
        validate_configured_service(service, configured)?;
        validate_credential_fields(service, configured)?;
        let container_id = service.observed_container_id().ok_or_else(|| {
            format!(
                "accepted v7 service '{}' has no exact Engine container",
                service.service_id()
            )
        })?;
        let source = V7LogicalDataMigrationSource::new(V7LogicalDataMigrationSourceOptions {
            project_id: accepted.project_id().to_owned(),
            service_id: service.service_id().to_owned(),
            kind: service.kind().to_owned(),
            driver: service.driver().to_owned(),
            container_name: service.container_name().to_owned(),
            container_id: container_id.to_owned(),
            named_volumes: accepted_named_volumes(service)?,
            logical_data: service.logical_data().clone(),
        })?;
        let credential = credential_for(*selection.adapter(), configured, service.logical_data())?;
        if credential.kind() != service.driver()
            && !(service.driver() == "valkey" && credential.kind() == "redis")
        {
            return Err(format!(
                "accepted v7 service '{}' credential kind differs from driver '{}'",
                service.service_id(),
                service.driver()
            ));
        }
        inputs.push(AcceptedV7LogicalDataInput::new(source, credential));
    }

    Ok(inputs)
}

fn load_exact_config(
    accepted: &AcceptedV7InventoryRecord,
    maximum_config_bytes: usize,
) -> Result<Config, String> {
    if maximum_config_bytes == 0 {
        return Err("accepted v7 config byte limit must be positive".to_owned());
    }
    let path = accepted.canonical_project_path().join(".stackctl.toml");
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| format!("failed to inspect accepted v7 config: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("accepted v7 config must remain a regular non-symlink file".to_owned());
    }
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("failed to read accepted v7 config: {error}"))?;
    if bytes.len() > maximum_config_bytes {
        return Err(format!(
            "accepted v7 config exceeds the {maximum_config_bytes} byte limit"
        ));
    }
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    if revision != accepted.source_revision() {
        return Err(
            "accepted v7 config changed after inventory acceptance; inventory and accept it again"
                .to_owned(),
        );
    }
    let config = load_config_with(LoadConfigPathOptions {
        config_path: Some(&path),
        project_root: Some(accepted.canonical_project_path()),
        runtime_env: None,
    })
    .map_err(|error| format!("accepted v7 config expansion failed: {error}"))?;
    let confirmed = std::fs::read(&path)
        .map_err(|error| format!("failed to re-read accepted v7 config: {error}"))?;
    if confirmed != bytes {
        return Err("accepted v7 config changed while credentials were resolved".to_owned());
    }
    let project_id = config
        .container_prefix
        .as_deref()
        .filter(|value| !value.is_empty() && *value != "stackctl")
        .map(str::to_owned)
        .or_else(|| {
            accepted
                .canonical_project_path()
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .ok_or_else(|| "accepted v7 project has no UTF-8 identity".to_owned())?;
    if project_id != accepted.project_id() {
        return Err("accepted v7 config project identity changed".to_owned());
    }

    Ok(config)
}

const fn logical_adapter(adapter: &V7MigrationServiceAdapter) -> bool {
    matches!(
        adapter,
        V7MigrationServiceAdapter::MongoDbLogicalDatabase
            | V7MigrationServiceAdapter::PostgresLogicalDatabase
            | V7MigrationServiceAdapter::MySqlLogicalDatabase
            | V7MigrationServiceAdapter::SqlServerLogicalDatabase
            | V7MigrationServiceAdapter::RedisTenantPrefix
            | V7MigrationServiceAdapter::ValkeyTenantPrefix
            | V7MigrationServiceAdapter::MinioBucket
            | V7MigrationServiceAdapter::RabbitMqVhost
    )
}

fn validate_credential_fields(
    accepted: &super::ipc::IpcV7ServiceInventory,
    configured: &ServiceConfig,
) -> Result<(), String> {
    let actual = [
        ("access_key", configured.access_key.is_some()),
        ("api_key", configured.api_key.is_some()),
        ("password", configured.password.is_some()),
        ("secret_key", configured.secret_key.is_some()),
        ("username", configured.username.is_some()),
    ]
    .into_iter()
    .filter_map(|(field, present)| present.then_some(field))
    .collect::<BTreeSet<_>>();
    let expected = accepted
        .credential_fields()
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if expected.len() != accepted.credential_fields().len() || actual != expected {
        return Err(format!(
            "accepted v7 service '{}' credential fields changed",
            accepted.service_id()
        ));
    }

    Ok(())
}

fn validate_configured_service(
    accepted: &super::ipc::IpcV7ServiceInventory,
    configured: &ServiceConfig,
) -> Result<(), String> {
    let driver = logical_driver(configured.driver).ok_or_else(|| {
        format!(
            "accepted v7 service '{}' is no longer a logical-data driver",
            accepted.service_id()
        )
    })?;
    let kind = match configured.kind {
        Kind::Database => "database",
        Kind::Cache => "cache",
        Kind::ObjectStore => "object_store",
        _ => {
            return Err(format!(
                "accepted v7 service '{}' is no longer a logical-data kind",
                accepted.service_id()
            ));
        }
    };
    if driver != accepted.driver()
        || kind != accepted.kind()
        || configured.image != accepted.configured_image()
    {
        return Err(format!(
            "accepted v7 service '{}' config identity differs from accepted evidence",
            accepted.service_id()
        ));
    }

    Ok(())
}

const fn logical_driver(driver: Driver) -> Option<&'static str> {
    Some(match driver {
        Driver::Mongodb => "mongodb",
        Driver::Postgres => "postgres",
        Driver::Mysql => "mysql",
        Driver::Sqlserver => "sqlserver",
        Driver::Redis => "redis",
        Driver::Valkey => "valkey",
        Driver::Minio => "minio",
        Driver::Rabbitmq => "rabbitmq",
        _ => return None,
    })
}

fn accepted_named_volumes(
    service: &super::ipc::IpcV7ServiceInventory,
) -> Result<Vec<String>, String> {
    let mut configured = service
        .configured_mounts()
        .iter()
        .filter(|mount| mount.source_kind() == "named_volume")
        .map(|mount| mount.source().to_owned())
        .collect::<Vec<_>>();
    let mut observed = service
        .observed_mounts()
        .iter()
        .filter(|mount| mount.source_kind() == "named_volume")
        .map(|mount| mount.source().to_owned())
        .collect::<Vec<_>>();
    configured.sort();
    observed.sort();
    if configured.iter().any(String::is_empty)
        || configured.windows(2).any(|pair| pair[0] == pair[1])
        || configured != observed
    {
        return Err(format!(
            "accepted v7 service '{}' named-volume evidence is inconsistent",
            service.service_id()
        ));
    }

    Ok(configured)
}

fn credential_for(
    adapter: V7MigrationServiceAdapter,
    service: &ServiceConfig,
    logical_data: &BTreeMap<String, String>,
) -> Result<V7LogicalDataCredential, String> {
    use V7MigrationServiceAdapter as Adapter;

    match adapter {
        Adapter::MongoDbLogicalDatabase => {
            Ok(V7LogicalDataCredential::MongoDb(V7MongoDbCredential::new(
                required(service, "username")?,
                required(service, "password")?,
                "admin",
            )?))
        }
        Adapter::PostgresLogicalDatabase => Ok(V7LogicalDataCredential::Postgres(
            V7PostgresCredential::new(
                required(service, "username")?,
                required(service, "password")?,
            )?,
        )),
        Adapter::MySqlLogicalDatabase => {
            Ok(V7LogicalDataCredential::MySql(V7MySqlCredential::new(
                required(service, "username")?,
                required(service, "password")?,
            )?))
        }
        Adapter::SqlServerLogicalDatabase => Ok(V7LogicalDataCredential::SqlServer(
            V7SqlServerCredential::new(
                required(service, "username")?,
                required(service, "password")?,
            )?,
        )),
        Adapter::RedisTenantPrefix | Adapter::ValkeyTenantPrefix => {
            let database = logical_data
                .get("database")
                .map(String::as_str)
                .unwrap_or("0")
                .parse::<u32>()
                .map_err(|error| format!("accepted Redis database is invalid: {error}"))?;
            Ok(V7LogicalDataCredential::Redis(V7RedisCredential::new(
                service.username.as_deref().unwrap_or("default"),
                service.password.as_deref().unwrap_or_default(),
                database,
            )?))
        }
        Adapter::MinioBucket => Ok(V7LogicalDataCredential::Minio(V7MinioCredential::new(
            required(service, "access_key")?,
            required(service, "secret_key")?,
        )?)),
        Adapter::RabbitMqVhost => Ok(V7LogicalDataCredential::RabbitMq(
            V7RabbitMqCredential::new(
                required(service, "username")?,
                required(service, "password")?,
            )?,
        )),
        _ => Err(format!(
            "v7 service '{}' is not a logical-data adapter",
            service.name
        )),
    }
}

fn required<'service>(
    service: &'service ServiceConfig,
    field: &str,
) -> Result<&'service str, String> {
    let value = match field {
        "username" => service.username.as_deref(),
        "password" => service.password.as_deref(),
        "access_key" => service.access_key.as_deref(),
        "secret_key" => service.secret_key.as_deref(),
        _ => None,
    };
    value.ok_or_else(|| {
        format!(
            "accepted v7 service '{}' requires configured {field}",
            service.name
        )
    })
}

fn one<'value, T>(values: Vec<&'value T>, description: &str) -> Result<&'value T, String> {
    match values.as_slice() {
        [value] => Ok(*value),
        [] => Err(format!("{description} is missing")),
        _ => Err(format!("{description} is ambiguous")),
    }
}
