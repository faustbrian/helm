/// Stable installation scope needed to materialize PostgreSQL demand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PostgresPreparationOptions<'operation> {
    pub(crate) installation_id: &'operation str,
    pub(crate) network_name: &'operation str,
    pub(crate) schema_version: u32,
}
