use crate::output::{LogLevel, Persistence};

pub(super) const fn default_persistence(level: LogLevel) -> Persistence {
    match level {
        LogLevel::Emergency
        | LogLevel::Alert
        | LogLevel::Critical
        | LogLevel::Error
        | LogLevel::Warn => Persistence::Persistent,
        LogLevel::Notice | LogLevel::Info | LogLevel::Debug | LogLevel::Success => {
            Persistence::Transient
        }
    }
}

#[cfg(test)]
mod tests {
    use super::default_persistence;
    use crate::output::{LogLevel, Persistence};

    #[test]
    fn warnings_and_errors_are_durable_without_call_site_annotations() {
        assert_eq!(default_persistence(LogLevel::Warn), Persistence::Persistent);
        assert_eq!(
            default_persistence(LogLevel::Error),
            Persistence::Persistent
        );
        assert_eq!(default_persistence(LogLevel::Info), Persistence::Transient);
        assert_eq!(default_persistence(LogLevel::Debug), Persistence::Transient);
    }
}
