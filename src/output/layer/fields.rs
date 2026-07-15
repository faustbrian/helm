//! Tracing event field collection and parsing helpers.

use serde_json::Value;

#[derive(Default)]
pub(super) struct FieldCollector {
    pub(super) body: Option<String>,
    pub(super) level: Option<String>,
    pub(super) context: Option<String>,
    pub(super) error: Option<String>,
    pub(super) persistence: Option<String>,
}

impl tracing::field::Visit for FieldCollector {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record(field.name(), value.to_owned());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record(field.name(), format!("{value:?}"));
    }
}

impl FieldCollector {
    fn record(&mut self, key: &str, value: String) {
        match key {
            "body" | "message" => self.body = Some(value),
            "error" => self.error = Some(value),
            "log_level" => self.level = Some(value),
            "context" => self.context = Some(value),
            "persistence" => self.persistence = Some(value),
            _ => {}
        }
    }

    pub(super) fn render_message(&self) -> String {
        match (&self.body, &self.error) {
            (Some(body), Some(error)) => format!("{body}: {error}"),
            (Some(body), None) => body.clone(),
            (None, Some(error)) => error.clone(),
            (None, None) => String::new(),
        }
    }
}

/// Parses context json into strongly typed values.
pub(super) fn parse_context_json(value: String) -> Option<Value> {
    serde_json::from_str::<Value>(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::FieldCollector;

    #[test]
    fn standard_tracing_messages_and_errors_remain_visible() {
        let mut fields = FieldCollector::default();
        fields.record("message", "application runtime build blocked".to_owned());
        fields.record("error", "extension installation failed".to_owned());

        assert_eq!(
            fields.render_message(),
            "application runtime build blocked: extension installation failed"
        );
    }
}
