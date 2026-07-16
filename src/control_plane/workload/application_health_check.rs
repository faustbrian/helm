use serde::Serialize;

/// Application-owned evidence required before a route is considered usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "path")]
pub(crate) enum ApplicationHealthCheck {
    Laravel,
    Tcp,
}

impl ApplicationHealthCheck {
    pub(crate) fn for_preset(preset: Option<&str>) -> Self {
        match preset {
            Some("laravel") => Self::Laravel,
            _ => Self::Tcp,
        }
    }
}
