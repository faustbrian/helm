//! Service label normalization helpers.

use crate::config;

pub(crate) fn kind_name(kind: config::Kind) -> String {
    format!("{kind:?}").to_lowercase()
}

pub(crate) fn driver_name(driver: config::Driver) -> String {
    format!("{driver:?}").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{driver_name, kind_name};
    use crate::config::{Driver, Kind};

    #[test]
    fn kind_name_is_lowercase_enum_name() {
        assert_eq!(kind_name(Kind::Database), "database");
        assert_eq!(kind_name(Kind::ObjectStore), "objectstore");
    }

    #[test]
    fn driver_name_is_lowercase_legacy_value() {
        assert_eq!(driver_name(Driver::Mysql), "mysql");
        assert_eq!(driver_name(Driver::Postgres), "postgres");
    }
}
