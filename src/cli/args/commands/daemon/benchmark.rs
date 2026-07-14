use super::BenchmarkEvidenceScenario;
use clap::Args;

/// Evidence constraints for one read-only benchmark sample.
#[derive(Args)]
pub(crate) struct DaemonBenchmarkArgs {
    /// Require the authoritative registry to contain exactly this many projects
    #[arg(long, value_name = "COUNT")]
    pub(crate) expect_projects: Option<usize>,
    /// Require one complete canonical benchmark fixture topology
    #[arg(
        long,
        value_enum,
        value_name = "SCENARIO",
        conflicts_with = "expect_projects"
    )]
    pub(crate) evidence_scenario: Option<BenchmarkEvidenceScenario>,
}
