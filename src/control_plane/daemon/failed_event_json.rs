use super::ipc::IpcEventKind;

const SERIALIZATION_FAILURE_EVENT_JSON: &str = concat!(
    r#"{"type":"failed","code":"event_serialization_failed","message":""#,
    r#"operation failed; detailed failure event could not be serialized"}"#,
);

/// Serializes a terminal failure without ever dropping its terminal event.
pub(crate) fn failed_event_json(code: &str, message: &str, operation_id: &str) -> String {
    let event = IpcEventKind::Failed {
        code: code.to_owned(),
        message: message.to_owned(),
    };

    serde_json::to_string(&event).unwrap_or_else(|error| {
        tracing::error!(
            operation_id,
            error = %error,
            "terminal failure event serialization failed"
        );

        SERIALIZATION_FAILURE_EVENT_JSON.to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::{IpcEventKind, SERIALIZATION_FAILURE_EVENT_JSON, failed_event_json};

    #[test]
    fn serializes_requested_failure() {
        let json = failed_event_json("backup_failed", "disk full", "operation-1");
        let event = serde_json::from_str::<IpcEventKind>(&json);

        assert!(matches!(
            event,
            Ok(IpcEventKind::Failed { code, message })
                if code == "backup_failed" && message == "disk full"
        ));
    }

    #[test]
    fn serialization_fallback_is_a_terminal_failure() {
        let event = serde_json::from_str::<IpcEventKind>(SERIALIZATION_FAILURE_EVENT_JSON);

        assert!(matches!(
            event,
            Ok(IpcEventKind::Failed { code, message })
                if code == "event_serialization_failed"
                    && message
                        == "operation failed; detailed failure event could not be serialized"
        ));
    }
}
