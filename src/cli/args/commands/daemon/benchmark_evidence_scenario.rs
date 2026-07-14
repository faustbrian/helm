use clap::ValueEnum;

/// A benchmark fixture topology whose identity must be proven before output.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum BenchmarkEvidenceScenario {
    V8One,
    V8FortyCompatible,
    V8FortySplit,
}
