use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Service-level Node toolchain preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct NodeToolchain {
    /// Preferred package manager for `helm node` and Node task workflows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_manager: Option<PackageManager>,
    /// Preferred Node version manager for app runtime commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_manager: Option<VersionManager>,
    /// Preferred Node version or alias such as `22` or `lts/*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Supported JS package managers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Bun,
    Npm,
    Pnpm,
    Yarn,
}

impl PackageManager {
    #[must_use]
    pub fn command_prefix(self) -> Vec<String> {
        match self {
            Self::Bun => vec!["bun".to_owned()],
            Self::Npm => vec!["npm".to_owned()],
            Self::Pnpm => vec!["pnpm".to_owned()],
            Self::Yarn => vec!["yarn".to_owned()],
        }
    }
}

/// Supported Node version managers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum VersionManager {
    System,
    Fnm,
    Nvm,
    Volta,
}

impl VersionManager {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Fnm => "fnm",
            Self::Nvm => "nvm",
            Self::Volta => "volta",
        }
    }
}
