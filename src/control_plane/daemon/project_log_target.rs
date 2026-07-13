/// Exact durable-to-Engine identity selected for one project-visible service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectLogTarget {
    service: String,
    resource_id: String,
    project_id: Option<String>,
}

impl ProjectLogTarget {
    pub(crate) fn new(service: String, resource_id: String, project_id: Option<String>) -> Self {
        Self {
            service,
            resource_id,
            project_id,
        }
    }

    pub(crate) fn service(&self) -> &str {
        &self.service
    }

    pub(crate) fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub(crate) fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }
}
