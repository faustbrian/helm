/// Stable installation scope needed to materialize MySQL-family demand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MySqlPreparationOptions<'operation> {
    pub(crate) installation_id: &'operation str,
    pub(crate) network_name: &'operation str,
    pub(crate) schema_version: u32,
}
