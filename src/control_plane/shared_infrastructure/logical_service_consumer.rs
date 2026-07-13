/// One project service logically isolated inside a shared instance.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct LogicalServiceConsumer {
    project_id: String,
    service_id: String,
}

impl LogicalServiceConsumer {
    pub(super) fn new(project_id: String, service_id: String) -> Self {
        Self {
            project_id,
            service_id,
        }
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }
}
