use super::RouteClaim;

/// A collision-free, canonical-path-deduplicated route registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedRouteRegistry {
    claims: Vec<RouteClaim>,
}

impl ValidatedRouteRegistry {
    pub(super) fn new(claims: Vec<RouteClaim>) -> Self {
        Self { claims }
    }

    /// Returns the validated claims in deterministic domain and path order.
    #[cfg(test)]
    pub(crate) fn claims(&self) -> &[RouteClaim] {
        &self.claims
    }
}
