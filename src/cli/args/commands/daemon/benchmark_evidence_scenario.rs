use clap::ValueEnum;

/// A benchmark fixture topology whose identity must be proven before output.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum BenchmarkEvidenceScenario {
    #[value(name = "v8-one")]
    One,
    #[value(name = "v8-forty-compatible")]
    FortyCompatible,
    #[value(name = "v8-forty-split")]
    FortySplit,
}
