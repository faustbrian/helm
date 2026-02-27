//! Object-store bucket bootstrap for runtime startup.

use anyhow::{Result, anyhow};
use std::time::Duration;

use crate::config::{Driver, ServiceConfig};

const AWS_CLI_IMAGE: &str = "amazon/aws-cli:latest";
const CONNECTIVITY_RETRY_ATTEMPTS: u32 = 8;
const CONNECTIVITY_RETRY_DELAY_MS: u64 = 250;

pub(super) fn ensure_bucket_exists(service: &ServiceConfig) -> Result<()> {
    if !is_object_store_driver(service.driver) {
        return Ok(());
    }

    let bucket = service.bucket.as_deref().unwrap_or("app");
    let endpoint = format!(
        "http://{}:{}",
        crate::docker::host_gateway_alias(),
        service.port
    );
    let access_key = service.access_key.as_deref().unwrap_or("minio");
    let secret_key = service.secret_key.as_deref().unwrap_or("miniosecret");
    let region = service.region.as_deref().unwrap_or("us-east-1");

    let head_args = build_aws_cli_args(
        "head-bucket",
        bucket,
        &endpoint,
        access_key,
        secret_key,
        region,
    );
    let create_args = build_aws_cli_create_args(bucket, &endpoint, access_key, secret_key, region);

    if crate::docker::is_dry_run() {
        crate::docker::print_docker_command(&head_args);
        crate::docker::print_docker_command(&create_args);
        return Ok(());
    }

    for attempt in 1..=CONNECTIVITY_RETRY_ATTEMPTS {
        let head = crate::docker::run_docker_output_owned(
            &head_args,
            &crate::docker::runtime_command_error_context("run"),
        )?;
        if head.status.success() {
            return Ok(());
        }
        let head_stderr = String::from_utf8_lossy(&head.stderr).trim().to_owned();

        let create = crate::docker::run_docker_output_owned(
            &create_args,
            &crate::docker::runtime_command_error_context("run"),
        )?;
        if create.status.success() {
            return Ok(());
        }

        let create_stderr = String::from_utf8_lossy(&create.stderr).trim().to_owned();
        if create_stderr.contains("BucketAlreadyOwnedByYou")
            || create_stderr.contains("BucketAlreadyExists")
        {
            return Ok(());
        }

        let retryable =
            is_connectivity_error(&head_stderr) || is_connectivity_error(&create_stderr);
        if retryable && attempt < CONNECTIVITY_RETRY_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(CONNECTIVITY_RETRY_DELAY_MS));
            continue;
        }

        let stderr = if create_stderr.is_empty() {
            head_stderr
        } else {
            create_stderr
        };
        return Err(anyhow!(
            "Failed to ensure object-store bucket '{}' for service '{}': {}",
            bucket,
            service.name,
            stderr
        ));
    }

    Err(anyhow!(
        "Failed to ensure object-store bucket '{}' for service '{}': endpoint was unreachable after {} attempts",
        bucket,
        service.name,
        CONNECTIVITY_RETRY_ATTEMPTS
    ))
}

fn is_object_store_driver(driver: Driver) -> bool {
    matches!(
        driver,
        Driver::Minio | Driver::Garage | Driver::Rustfs | Driver::Localstack
    )
}

fn is_connectivity_error(stderr: &str) -> bool {
    stderr.contains("Could not connect to the endpoint URL")
        || stderr.contains("Connect timeout on endpoint URL")
}

fn build_aws_cli_args(
    operation: &str,
    bucket: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
) -> Vec<String> {
    let mut args = vec!["run".to_owned(), "--rm".to_owned()];
    if let Some(mapping) = crate::docker::host_gateway_mapping() {
        args.push("--add-host".to_owned());
        args.push(mapping.to_owned());
    }
    args.extend([
        "-e".to_owned(),
        format!("AWS_ACCESS_KEY_ID={access_key}"),
        "-e".to_owned(),
        format!("AWS_SECRET_ACCESS_KEY={secret_key}"),
        "-e".to_owned(),
        format!("AWS_DEFAULT_REGION={region}"),
        AWS_CLI_IMAGE.to_owned(),
        "s3api".to_owned(),
        operation.to_owned(),
        "--bucket".to_owned(),
        bucket.to_owned(),
        "--endpoint-url".to_owned(),
        endpoint.to_owned(),
    ]);
    args
}

fn build_aws_cli_create_args(
    bucket: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
) -> Vec<String> {
    let mut args = build_aws_cli_args(
        "create-bucket",
        bucket,
        endpoint,
        access_key,
        secret_key,
        region,
    );
    if region != "us-east-1" {
        args.push("--create-bucket-configuration".to_owned());
        args.push(format!("LocationConstraint={region}"));
    }
    args
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config::{Driver, Kind, ServiceConfig};

    use super::ensure_bucket_exists;

    static UNIQUE_SUFFIX_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_suffix() -> String {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0_u128, |dur| dur.as_nanos());
        let nonce = UNIQUE_SUFFIX_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{stamp}-{nonce}")
    }

    fn service(driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: "s3".to_owned(),
            kind: Kind::ObjectStore,
            driver,
            image: "rustfs/rustfs:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
            database: None,
            username: None,
            password: None,
            bucket: Some("media".to_owned()),
            access_key: Some("minio".to_owned()),
            secret_key: Some("miniosecret".to_owned()),
            api_key: None,
            region: Some("us-east-1".to_owned()),
            scheme: None,
            domain: None,
            domains: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            localhost_tls: false,
            octane: false,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            container_name: Some("acme-s3".to_owned()),
            resolved_container_name: Some("acme-s3".to_owned()),
        }
    }

    fn with_fake_runtime_command<T>(script: &str, test: impl FnOnce() -> T) -> T {
        let dir = std::env::temp_dir().join(format!("helm-fake-runtime-{}", unique_suffix()));
        fs::create_dir_all(&dir).expect("create temp runtime dir");
        let binary = dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake runtime binary");
        writeln!(file, "#!/bin/sh\n{script}").expect("write fake runtime script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod fake runtime binary");
        }
        let runtime_bin = binary.to_string_lossy().to_string();

        let result = crate::docker::with_docker_command(&runtime_bin, test);
        fs::remove_dir_all(&dir).ok();
        result
    }

    #[test]
    fn ensure_bucket_exists_skips_non_object_store_drivers() {
        with_fake_runtime_command("exit 99", || {
            let mut svc = service(Driver::Mysql);
            svc.kind = Kind::Database;
            ensure_bucket_exists(&svc).expect("should skip non object-store service");
        });
    }

    #[test]
    fn ensure_bucket_exists_creates_bucket_when_missing() {
        with_fake_runtime_command(
            r#"
if echo "$*" | grep -q "head-bucket"; then
  echo "Not Found" 1>&2
  exit 1
fi
if echo "$*" | grep -q "create-bucket"; then
  exit 0
fi
exit 1
"#,
            || {
                ensure_bucket_exists(&service(Driver::Rustfs))
                    .expect("should create missing object-store bucket");
            },
        );
    }

    #[test]
    fn ensure_bucket_exists_tolerates_bucket_already_owned_on_create() {
        with_fake_runtime_command(
            r#"
if echo "$*" | grep -q "head-bucket"; then
  echo "Not Found" 1>&2
  exit 1
fi
if echo "$*" | grep -q "create-bucket"; then
  echo "BucketAlreadyOwnedByYou" 1>&2
  exit 1
fi
exit 1
"#,
            || {
                ensure_bucket_exists(&service(Driver::Minio))
                    .expect("already-owned bucket should not fail");
            },
        );
    }

    #[test]
    fn ensure_bucket_exists_uses_host_gateway_alias_endpoint() {
        let log = std::env::temp_dir().join(format!("helm-bucket-args-{}.log", unique_suffix()));
        let log_path = log.to_string_lossy().to_string();
        with_fake_runtime_command(
            &format!(
                r#"
echo "$*" >> "{}"
if echo "$*" | grep -q "head-bucket"; then
  exit 0
fi
exit 1
"#,
                log_path
            ),
            || {
                ensure_bucket_exists(&service(Driver::Rustfs)).expect("bucket check should pass");
            },
        );

        let content = fs::read_to_string(Path::new(&log)).expect("read fake runtime log");
        assert!(content.contains("--endpoint-url http://host.docker.internal:9000"));
        fs::remove_file(log).ok();
    }

    #[test]
    fn ensure_bucket_exists_adds_host_gateway_mapping_for_docker() {
        let log =
            std::env::temp_dir().join(format!("helm-bucket-host-map-{}.log", unique_suffix()));
        let log_path = log.to_string_lossy().to_string();
        with_fake_runtime_command(
            &format!(
                r#"
echo "$*" >> "{}"
if echo "$*" | grep -q "head-bucket"; then
  exit 0
fi
exit 1
"#,
                log_path
            ),
            || {
                ensure_bucket_exists(&service(Driver::Rustfs)).expect("bucket check should pass");
            },
        );

        let content = fs::read_to_string(Path::new(&log)).expect("read fake runtime log");
        assert!(content.contains("--add-host host.docker.internal:host-gateway"));
        fs::remove_file(log).ok();
    }

    #[test]
    fn ensure_bucket_exists_retries_connectivity_failures() {
        let counter =
            std::env::temp_dir().join(format!("helm-bucket-connect-retry-{}.cnt", unique_suffix()));
        let counter_path = counter.to_string_lossy().to_string();
        with_fake_runtime_command(
            &format!(
                r#"
count=$(cat "{}" 2>/dev/null || echo 0)
count=$((count + 1))
echo "$count" > "{}"
if [ "$count" -lt 3 ]; then
  echo "Connect timeout on endpoint URL: http://host.docker.internal:9000/media" 1>&2
  exit 1
fi
if echo "$*" | grep -q "head-bucket"; then
  exit 0
fi
exit 1
"#,
                counter_path, counter_path
            ),
            || {
                ensure_bucket_exists(&service(Driver::Rustfs))
                    .expect("transient connectivity should be retried");
            },
        );

        fs::remove_file(counter).ok();
    }
}
