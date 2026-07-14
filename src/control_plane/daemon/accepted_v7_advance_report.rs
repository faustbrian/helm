/// Independent outcomes from one reconciliation pass over accepted v7 plans.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct AcceptedV7AdvanceReport {
    advanced: usize,
    issues: Vec<String>,
}

impl AcceptedV7AdvanceReport {
    pub(super) const fn new(advanced: usize, issues: Vec<String>) -> Self {
        Self { advanced, issues }
    }

    pub(crate) const fn advanced(&self) -> usize {
        self.advanced
    }

    pub(crate) fn issues(&self) -> &[String] {
        &self.issues
    }
}
