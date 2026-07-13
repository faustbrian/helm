mod config_parse_error;
mod parse_project_config;
mod project_config_schema;
mod raw_project_config;
mod raw_service_config;

pub(crate) use config_parse_error::ConfigParseError;
pub(crate) use parse_project_config::parse_project_config;
pub(crate) use project_config_schema::project_config_schema;
pub(crate) use raw_project_config::RawProjectConfig;
pub(crate) use raw_service_config::RawServiceConfig;

#[cfg(test)]
mod tests;
