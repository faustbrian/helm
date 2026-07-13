/// Stable installation scope for stateless Gotenberg demand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GotenbergPreparationOptions<'operation> {
    pub(crate) installation_id: &'operation str,
    pub(crate) network_name: &'operation str,
    pub(crate) schema_version: u32,
}
