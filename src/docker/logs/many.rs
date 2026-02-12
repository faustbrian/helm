use anyhow::Result;

use crate::config::ServiceConfig;

use super::{logs, logs_prefixed};

pub(super) fn logs_many(
    services: &[ServiceConfig],
    follow: bool,
    tail: Option<u64>,
    timestamps: bool,
    prefix: bool,
) -> Result<()> {
    if services.is_empty() {
        return Ok(());
    }

    if !follow {
        for service in services {
            if prefix {
                logs_prefixed(service, false, tail, timestamps)?;
            } else {
                println!("== {} ==", service.name);
                logs(service, false, tail, timestamps)?;
            }
        }
        return Ok(());
    }

    let handles: Vec<std::thread::JoinHandle<Result<()>>> = services
        .iter()
        .cloned()
        .map(|service| {
            std::thread::spawn(move || {
                if prefix {
                    logs_prefixed(&service, true, tail, timestamps)
                } else {
                    logs(&service, true, tail, timestamps)
                }
            })
        })
        .collect();

    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| anyhow::anyhow!("failed joining logs thread"))?;
        result?;
    }

    Ok(())
}
