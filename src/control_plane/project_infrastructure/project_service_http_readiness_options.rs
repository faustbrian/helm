/// Credential transport for one private HTTP readiness request.
pub(super) enum ProjectServiceHttpAuthentication<'credential> {
    Basic {
        username: &'static str,
        environment_key: &'static str,
        secret: &'credential str,
    },
    Bearer {
        environment_key: &'static str,
        secret: &'credential str,
    },
    Header {
        header_name: &'static str,
        environment_key: &'static str,
        secret: &'credential str,
    },
}

/// Inputs for one pinned-client authenticated HTTP readiness job.
pub(super) struct ProjectServiceHttpReadinessOptions<'credential> {
    pub(super) url: String,
    pub(super) authentication: ProjectServiceHttpAuthentication<'credential>,
    pub(super) allow_invalid_certificate: bool,
}
