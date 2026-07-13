/// One opaque daemon lifecycle event retained by transactional state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DaemonEventRecord {
    sequence: u64,
    operation_id: String,
    kind_json: String,
}

impl DaemonEventRecord {
    pub(crate) const fn new(sequence: u64, operation_id: String, kind_json: String) -> Self {
        Self {
            sequence,
            operation_id,
            kind_json,
        }
    }

    pub(crate) const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) fn kind_json(&self) -> &str {
        &self.kind_json
    }
}
