use crate::config::{Driver, ServiceConfig};

pub(super) fn append(args: &mut Vec<String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Meilisearch => {
            if let Some(api_key) = &service.api_key {
                args.push("-e".to_owned());
                args.push(format!("MEILI_MASTER_KEY={api_key}"));
            }
        }
        Driver::Typesense => {
            if let Some(api_key) = &service.api_key {
                args.push("-e".to_owned());
                args.push(format!("TYPESENSE_API_KEY={api_key}"));
            }
        }
        _ => {}
    }
}
