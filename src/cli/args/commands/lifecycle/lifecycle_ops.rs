//! cli args commands lifecycle lifecycle ops module.
//!
//! Contains cli args commands lifecycle lifecycle ops logic used by Helm command workflows.

use clap::Args;

use crate::cli::args::default_parallelism;
use crate::config;

#[derive(Args)]
pub(crate) struct DownArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Skip stopping workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Allow downing shared workspace dependencies
    #[arg(long, short, default_value_t = false, conflicts_with = "no_deps")]
    pub(crate) force: bool,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl DownArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}

#[derive(Args)]
pub(crate) struct StopArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl StopArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}

#[derive(Args)]
pub(crate) struct RmArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, short, default_value_t = false)]
    pub(crate) force: bool,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl RmArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}

#[derive(Args)]
pub(crate) struct RecreateArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Wait for service(s) to accept connections after recreating
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    /// Timeout in seconds for --wait
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    /// Publish all exposed ports to random host ports after start
    #[arg(long, short = 'P', default_value_t = false)]
    pub(crate) publish_all: bool,
    /// Persist random port assignments into `.helm.toml`
    #[arg(long, default_value_t = false, requires = "publish_all")]
    pub(crate) save_ports: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl RecreateArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}

#[derive(Args)]
pub(crate) struct RestartArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl RestartArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}

#[derive(Args)]
pub(crate) struct RelabelArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Wait for service(s) to accept connections after relabeling
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    /// Timeout in seconds for --wait
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl RelabelArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::args::commands::lifecycle::lifecycle_ops::{
        DownArgs, RecreateArgs, RelabelArgs, RestartArgs, RmArgs, StopArgs,
    };
    use crate::cli::args::default_parallelism;
    use crate::config::Kind;

    #[test]
    fn getters_return_service_filter() {
        let args = DownArgs {
            service: Some("api".to_owned()),
            kind: Some(Kind::App),
            no_deps: false,
            force: false,
            parallel: 1,
        };

        assert_eq!(args.service(), Some("api"));
        assert_eq!(args.kind(), Some(Kind::App));
        assert!(!args.no_deps);
    }

    #[test]
    fn helper_defaults_are_present() {
        let args = StopArgs {
            service: None,
            kind: None,
            parallel: default_parallelism(),
        };
        assert_eq!(args.service(), None);
        assert_eq!(args.parallel, default_parallelism());
        assert_eq!(args.kind(), None);
    }

    #[test]
    fn other_lifecycle_getters_expose_parsed_fields() {
        let args = RmArgs {
            service: None,
            kind: Some(Kind::Database),
            force: true,
            parallel: 2,
        };
        assert_eq!(args.kind(), Some(Kind::Database));
        assert!(args.force);

        let recreate = RecreateArgs {
            service: Some("db".to_owned()),
            kind: Some(Kind::Database),
            wait: true,
            wait_timeout: 5,
            publish_all: false,
            save_ports: false,
            env_output: true,
            parallel: 1,
        };
        assert_eq!(recreate.service(), Some("db"));
        assert!(recreate.wait);
        assert_eq!(recreate.wait_timeout, 5);

        let restart = RestartArgs {
            service: Some("redis".to_owned()),
            kind: Some(Kind::Cache),
            wait: false,
            wait_timeout: 10,
            parallel: 4,
        };
        assert_eq!(restart.service(), Some("redis"));

        let relabel = RelabelArgs {
            service: None,
            kind: None,
            wait: false,
            wait_timeout: 1,
            parallel: 4,
        };
        assert_eq!(relabel.kind(), None);
    }
}
