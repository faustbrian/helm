/// Stable identity and Engine context for preparing one migration target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PostgresMigrationPreparationOptions<'operation> {
    pub(crate) migration_id: &'operation str,
    pub(crate) project_id: &'operation str,
    pub(crate) installation_id: &'operation str,
    pub(crate) network_name: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: &'operation str,
}
