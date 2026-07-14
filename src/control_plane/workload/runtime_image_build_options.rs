/// Complete immutable inputs for one reusable application runtime build.
pub(crate) struct RuntimeImageBuildOptions<'value> {
    pub(crate) installation_id: &'value str,
    pub(crate) schema_version: u32,
    pub(crate) base_image_digest: &'value str,
    pub(crate) platform: &'value str,
    pub(crate) php_extensions: Vec<String>,
    pub(crate) composer_image: Option<&'value str>,
    pub(crate) node_image: Option<&'value str>,
    pub(crate) bun_image: Option<&'value str>,
}
