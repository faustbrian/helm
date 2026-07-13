use std::fmt::{Display, Formatter};

/// Last durable operation completed by one reversible resource migration.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum MigrationPhase {
    Inventoried,
    BackupVerified,
    TargetProvisioned,
    DataRestored,
    TargetVerified,
    Cutover,
    Confirmed,
    RolledBack,
}

impl MigrationPhase {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Inventoried => "inventoried",
            Self::BackupVerified => "backup_verified",
            Self::TargetProvisioned => "target_provisioned",
            Self::DataRestored => "data_restored",
            Self::TargetVerified => "target_verified",
            Self::Cutover => "cutover",
            Self::Confirmed => "confirmed",
            Self::RolledBack => "rolled_back",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "inventoried" => Some(Self::Inventoried),
            "backup_verified" => Some(Self::BackupVerified),
            "target_provisioned" => Some(Self::TargetProvisioned),
            "data_restored" => Some(Self::DataRestored),
            "target_verified" => Some(Self::TargetVerified),
            "cutover" => Some(Self::Cutover),
            "confirmed" => Some(Self::Confirmed),
            "rolled_back" => Some(Self::RolledBack),
            _ => None,
        }
    }

    pub(crate) const fn can_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Inventoried, Self::BackupVerified)
                | (Self::BackupVerified, Self::TargetProvisioned)
                | (Self::TargetProvisioned, Self::DataRestored)
                | (Self::DataRestored, Self::TargetVerified)
                | (Self::TargetVerified, Self::Cutover)
                | (Self::Cutover, Self::Confirmed)
        ) || (!matches!(self, Self::Confirmed | Self::RolledBack)
            && matches!(next, Self::RolledBack))
    }
}

impl Display for MigrationPhase {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}
