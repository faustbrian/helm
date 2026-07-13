use std::collections::BTreeMap;

/// Synchronous daemon boundary for one bounded set of registry lookups.
pub(crate) trait ImageReferenceResolution {
    fn resolve(
        &mut self,
        references: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, String>;
}
