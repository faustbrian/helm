/// One project-scoped logical resource that failed exact convergence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogicalResourceDrift {
    resource_id: String,
    detail: String,
}

impl LogicalResourceDrift {
    pub(crate) fn new(resource_id: String, detail: String) -> Self {
        Self {
            resource_id,
            detail,
        }
    }

    pub(crate) fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub(crate) fn detail(&self) -> &str {
        &self.detail
    }
}
