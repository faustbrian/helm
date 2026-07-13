/// Supported independently fingerprinted MySQL-compatible implementations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MySqlFlavor {
    MySql,
    MariaDb,
}

impl MySqlFlavor {
    pub(super) fn from_implementation(implementation: &str) -> Option<Self> {
        match implementation {
            "mysql" => Some(Self::MySql),
            "mariadb" => Some(Self::MariaDb),
            _ => None,
        }
    }

    pub(super) const fn implementation(self) -> &'static str {
        match self {
            Self::MySql => "mysql",
            Self::MariaDb => "mariadb",
        }
    }

    pub(super) const fn root_password_key(self) -> &'static str {
        match self {
            Self::MySql => "MYSQL_ROOT_PASSWORD",
            Self::MariaDb => "MARIADB_ROOT_PASSWORD",
        }
    }

    pub(super) const fn client_executable(self) -> &'static str {
        match self {
            Self::MySql => "mysql",
            Self::MariaDb => "mariadb",
        }
    }
}
