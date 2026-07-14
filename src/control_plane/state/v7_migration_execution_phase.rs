use serde::{Deserialize, Serialize};

/// Durable project-wide barrier for one accepted v7 migration plan.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum V7MigrationExecutionPhase {
    Planned,
    Preparing,
    Prepared,
    Cutover,
    Confirmed,
    RolledBack,
}

impl V7MigrationExecutionPhase {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Preparing => "preparing",
            Self::Prepared => "prepared",
            Self::Cutover => "cutover",
            Self::Confirmed => "confirmed",
            Self::RolledBack => "rolled_back",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "planned" => Some(Self::Planned),
            "preparing" => Some(Self::Preparing),
            "prepared" => Some(Self::Prepared),
            "cutover" => Some(Self::Cutover),
            "confirmed" => Some(Self::Confirmed),
            "rolled_back" => Some(Self::RolledBack),
            _ => None,
        }
    }

    pub(crate) fn can_advance_to(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (Self::Planned, Self::Preparing)
                    | (Self::Preparing, Self::Prepared)
                    | (Self::Prepared, Self::Cutover)
                    | (Self::Cutover, Self::Confirmed)
            )
            || (!matches!(self, Self::Confirmed | Self::RolledBack)
                && matches!(next, Self::RolledBack))
    }
}
